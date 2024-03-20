use std::borrow::{Borrow, Cow};
use std::collections::{HashMap, VecDeque};

use petgraph::prelude::{DfsPostOrder, NodeIndex};
use petgraph::{algo, Directed, Graph};
use serde::de;

use crate::error::Error;
use crate::package::index::PackageIndex;
use crate::ts::experimental::index::PackageIndexEntry;
use crate::ts::package_reference::PackageReference;
use crate::ts::version::Version;
use crate::TCLI_HOME;

pub type InnerDepGraph = Graph<PackageReference, (), Directed>;

pub enum Granularity {
    All,
    IgnoreVersion,
    LesserVersion,
    GreaterVersion,
}

#[derive(Debug)]
pub struct GraphDelta {
    pub add: Vec<PackageReference>,
    pub del: Vec<PackageReference>,
}

pub struct DependencyGraph {
    graph: InnerDepGraph,
    index: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        // Initialize the graph with a root node at index 0.
        let mut graph = InnerDepGraph::new();
        let dummy_ref = PackageReference::new("@", "@", Version::new(0, 0, 0)).unwrap();
        graph.add_node(dummy_ref);

        DependencyGraph {
            graph,
            index: HashMap::new(),
        }
    }

    pub fn from_graph(graph: InnerDepGraph) -> Self {
        let index = graph
            .node_indices()
            .map(|x| ((graph[x]).to_loose_ident_string(), x))
            .collect::<HashMap<String, NodeIndex>>();

        DependencyGraph {
            graph,
            index
        }
    }

    pub fn into_inner(self) -> InnerDepGraph {
        self.graph
    }

    /// Add a node to the dependency graph, replacing if it already exists within the graph
    /// but is of a lesser semver.
    pub fn add(&mut self, value: PackageReference) {
        let node_index = *self
            .index
            .entry(value.to_loose_ident_string())
            .or_insert_with(|| self.graph.add_node(value.clone()));

        let graph_value = &self.graph[node_index];

        if graph_value.version < value.version {
            self.graph[node_index] = value;
        }
    }

    /// Add an edge between two values in the graph.
    pub fn add_edge(&mut self, parent: &PackageReference, child: &PackageReference) {
        let parent_index = self.index[&parent.to_loose_ident_string()];
        let child_index = self.index[&child.to_loose_ident_string()];

        self.graph.add_edge(parent_index, child_index, ());
    }

    pub fn add_rooted_edge(&mut self, child: &PackageReference) {
        let child_index = self.index[&child.to_loose_ident_string()];
        self.graph.add_edge(NodeIndex::from(0), child_index, ());
    }

    /// Determine if the given value exists within the graph within the specified granularity.
    pub fn exists(&self, value: &PackageReference, granularity: Granularity) -> bool {
        let loose = value.to_loose_ident_string();
        let node_index = self.index.get(&loose);

        if node_index.is_none() {
            return false;
        }

        let node_index = node_index.unwrap();
        let graph_value = &self.graph[*node_index];

        match granularity {
            Granularity::All => graph_value.version == value.version,
            Granularity::IgnoreVersion => true,
            Granularity::LesserVersion => graph_value.version < value.version,
            Granularity::GreaterVersion => graph_value.version > value.version,
        }
    }

    /// Get the dependencies of value's node within the graph.
    ///
    /// The resultant Vec is not guarenteed to be in "install order"; instead it is ordered
    /// by traversal cost.
    pub fn get_dependencies(&self, value: &PackageReference) -> Option<Vec<&PackageReference>> {
        let loose = value.to_loose_ident_string();
        let node_index = self.index.get(&loose)?;

        // Compute the shortest path to every child node.
        let mut children = algo::dijkstra(&self.graph, *node_index, None, |_| 1)
            .into_iter()
            .map(|(index, cost)| (&self.graph[index], cost))
            .collect::<Vec<_>>();

        // Sort the children by their cost, which describes the number of "steps" that were required
        // to path to each node.
        children.sort_by(|first, second| first.1.cmp(&second.1));

        Some(
            children
                .into_iter()
                .map(|(package_ref, _)| package_ref)
                .collect::<Vec<_>>(),
        )
    }

    /// Digest the dependency graph, resolving its contents into a DFS-ordered list of package references.
    pub fn digest(&self) -> Vec<&PackageReference> {
        let mut dfs = DfsPostOrder::new(&self.graph, NodeIndex::new(0));
        let mut dependencies = Vec::new();

        while let Some(element) = dfs.next(&self.graph) {
            if element.index() == 0 {
                continue;
            }

            dependencies.push(&self.graph[element]);
        }

        dependencies
    }

    pub fn graph_delta(&self, other: &DependencyGraph) -> GraphDelta {
        // Create lookup tables for self.graph and other.graph.
        // These tables map loose identifier strings to (index, value) tuples.
        let self_table = self
            .digest()
            .into_iter()
            .enumerate()
            .map(|(i, x)| (x.to_loose_ident_string(), (i, x)))
            .collect::<HashMap<_, _>>();

        let other_table = other
            .digest()
            .into_iter()
            .enumerate()
            .map(|(i, x)| (x.to_loose_ident_string(), (i, x)))
            .collect::<HashMap<_, _>>();

        let mut add = vec![];
        let mut del = vec![];

        // Handle the following cases:
        // - self_value.version < other_value.version => ADD other_value + DEL self_value
        // - !other_table.contains(this_value) => DEL self_value
        for (key, (self_index, self_value)) in self_table.iter() {
            let other = other_table.get(key);

            match other {
                Some((other_index, other_value)) if self_value.version < other_value.version => {
                    add.push((other_index, (*other_value).clone()));
                    del.push((self_index, (*self_value).clone()));
                },
                Some(_) => (),
                None => del.push((self_index, (*self_value).clone())),
            }
        }

        // Handle the remaining case:
        // - !this_table.contains(other_value) => ADD other_value
        add.extend(other_table
            .iter()
            .filter_map(|(key, (other_index, other_value))| match self_table.get(key) {
                Some(_) => None,
                None => Some((other_index, (*other_value).clone()))
            }));

        // Sort entries by their index (i, _) to maintain order.
        add.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
        del.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());

        GraphDelta {
            add: add
                .into_iter()
                .map(|(_, x)| x)
                .collect::<Vec<_>>(),
            del: del
                .into_iter()
                .map(|(_, x)| x)
                .collect::<Vec<_>>(),
        }
    }
}

// type DependencyGraph<'a> = Graph::<&'a PackageReference, (), Directed>;

/// Generate a deduplicated list of package dependencies. This describes every package that
/// needs to be downloaded and installed for all of the root packages to function.
///
/// This takes into account:
/// 1. Packages already installed into the project.
/// 2. Dependencies specified within local packages within the cache.
/// 3. Dependencies specified within the remote repository.
pub async fn resolve_packages(packages: Vec<PackageReference>) -> Result<DependencyGraph, Error> {
    let start = std::time::Instant::now();
    let package_index = PackageIndex::open(&TCLI_HOME).await?;

    let mut graph = DependencyGraph::new();
    let mut iter_queue: VecDeque<Cow<PackageReference>> =
        VecDeque::from(packages.iter().map(Cow::Borrowed).collect::<Vec<_>>());

    while let Some(package_ident) = iter_queue.pop_front() {
        let package = package_index
            .get_package(package_ident.as_ref())
            .unwrap_or_else(|| panic!("{} does not exist in the index.", package_ident));

        // Add the package to the dependency graph.
        graph.add(package_ident.clone().into_owned());

        for dependency in package.dependencies.into_iter() {
            let dependency = Cow::Owned(dependency);

            // Queue up this dependency for processing if:
            // 1. This dependency already exists within the graph, but is a lesser version.
            // 2. This dependency does not exist within the graph.
            if !graph.exists(&dependency, Granularity::GreaterVersion) {
                let inner = &dependency;

                graph.add(dependency.clone().into_owned());
                graph.add_edge(package_ident.as_ref(), inner);

                iter_queue.push_back(dependency);
            } else {
                // Split this up into an if/else to extend the lifetime of the Cow.
                graph.add_edge(package_ident.as_ref(), &dependency);
            }
        }
    }

    for package in packages {
        graph.add_rooted_edge(&package);
    }

    let packages = graph.digest();

    let pkg_count = packages.len();

    println!("Resolved {} packages in {}ms", pkg_count, start.elapsed().as_millis());

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::str::FromStr;
    use std::sync::Once;

    use super::*;
    use crate::package::resolver;

    static INIT: Once = Once::new();

    fn init() {
        INIT.call_once(|| {
            crate::ts::init_repository("https://thunderstore.io", None);
        })
    }

    #[tokio::test]
    /// Test the resolver's general ability to resolve package dependencies.
    async fn test_resolver() {
        init();

        let expected = {
            let expected = vec![
                "bbepis-BepInExPack-5.4.2113",
                "RiskofThunder-BepInEx_GUI-3.0.1",
                "RiskofThunder-FixPluginTypesSerialization-1.0.3",
                "RiskofThunder-RoR2BepInExPack-1.9.0",
            ];

            expected
                .into_iter()
                .map(|x| PackageReference::from_str(x).unwrap())
                .collect::<HashSet<_>>()
        };

        let target = PackageReference::from_str("bbepis-BepInExPack-5.4.2113").unwrap();
        let got = resolver::resolve_packages(vec![target]).await.unwrap();

        for package in got.digest().iter() {
            assert!(expected.contains(package));
        }
    }

    #[tokio::test]
    /// Test the resolver's ability to handle version collisions.
    async fn test_resolver_version_hiearchy() {
        init();

        let expected = {
            let expected = vec![
                "bbepis-BepInExPack-5.4.2113",
                "RiskofThunder-BepInEx_GUI-3.0.1",
                "RiskofThunder-FixPluginTypesSerialization-1.0.3",
                "RiskofThunder-RoR2BepInExPack-1.9.0",
            ];

            expected
                .into_iter()
                .map(|x| PackageReference::from_str(x).unwrap())
                .collect::<HashSet<_>>()
        };

        let target = PackageReference::from_str("bbepis-BepInExPack-5.4.2113").unwrap();
        let disrupt = PackageReference::from_str("bbepis-BepInExPack-5.4.2112").unwrap();

        let graph = resolver::resolve_packages(vec![target, disrupt])
            .await
            .unwrap();
        let got = graph.digest();

        for package in got.iter() {
            assert!(expected.contains(package));
        }

        assert_eq!(expected.len(), got.len());
    }
}
