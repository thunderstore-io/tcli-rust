use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub enum OS {
    Windows,
    Mac,
    Linux,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]

pub enum Arch {
    X86_64,
    X86,
    AArch64,
    Arm,
}

impl Display for OS {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str_name = match self {
            OS::Windows => "Windows",
            OS::Mac => "Mac",
            OS::Linux => "Linux",
        };

        write!(f, "{str_name}")
    }
}

impl From<String> for OS {
    fn from(value: String) -> Self {
        let lowercase = value.to_lowercase();

        match lowercase.as_str() {
            "windows" => OS::Windows,
            "macos" => OS::Mac,
            "linux" => OS::Linux,
            _ => panic!("'{value}' is not a valid OS name."),
        }
    }
}

impl Display for Arch {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str_name = match self {
            Arch::X86_64 => "x86_64",
            Arch::X86 => "x86",
            Arch::AArch64 => "aarch64",
            Arch::Arm => "arm",
        };

        write!(f, "{str_name}")
    }
}

impl From<String> for Arch {
    fn from(value: String) -> Self {
        let lowercase = value.to_lowercase();

        match lowercase.as_str() {
            "x86_64" => Arch::X86_64,
            "x86" => Arch::X86,
            "aarch64" => Arch::AArch64,
            "arm" => Arch::Arm,
            _ => panic!("'{value}' is not a valid architecture name."),
        }
    }
}
