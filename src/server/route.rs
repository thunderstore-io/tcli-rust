use std::sync::mpsc::Sender;

use self::package::PackageMethod;
use self::project::ProjectMethod;

use super::Error;
use super::method::*;
use super::proto::{Request, Response};

/// Route the request to the appropriate TCLI backend.
pub fn route(request: Request) -> Result<(), ()> {
    match request.method {
        Method::Package(PackageMethod::IsCached) => panic!(),
        _ => panic!(),
    };

    Ok(())
}

fn route_project(method: ProjectMethod, tx: Sender<Result<Response, Error>>) {
    match method {
        ProjectMethod::SetContext(_) => todo!(),
        ProjectMethod::ReleaseContext => todo!(),
        ProjectMethod::GetMetadata => todo!(),
        ProjectMethod::AddPackages(_) => todo!(),
        ProjectMethod::RemovePackages(_) => todo!(),
        ProjectMethod::GetPackages => todo!(),
        ProjectMethod::IsValid => todo!(),
    }
}

fn route_package(method: PackageMethod) {
    match method {
        PackageMethod::IsCached => (),
        PackageMethod::GetMetadata => (),
    }
}
