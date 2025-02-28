#![deny(missing_docs)]

//! A library to detect Conda virtual packages present on a system.
//!
//! A virtual package represents a package that is injected into the solver to provide system
//! information to packages. This allows packages to add dependencies on specific system features,
//! like the platform version, the machines architecture, or the availability of a Cuda driver
//! with a specific version.
//!
//! This library provides both a low- and high level API to detect versions of virtual packages for
//! the host system.
//!
//! To detect all virtual packages for the host system use the [`VirtualPackage::current`] method
//! which will return a memoized slice of all detected virtual packages. The `VirtualPackage` enum
//! represents all available virtual package types. Using it provides some flexibility to the
//! user to not care about which exact virtual packages exist but still allows users to override
//! specific virtual package behavior. Say for instance you just want to detect the capabilities of
//! the host system but you still want to restrict the targeted linux version. You can convert an
//! instance of `VirtualPackage` to `GenericVirtualPackage` which erases any typing for specific
//! virtual packages.
//!
//! Each virtual package is also represented by a struct which can be used to detect the specifics
//! of one virtual package. For instance the [`Linux::current`] method returns an instance of
//! `Linux` which contains the current Linux version. It also provides conversions to the higher
//! level API.
//!
//! Finally at the core of the library are detection functions to perform specific capability
//! detections that are not tied to anything related to virtual packages. See
//! [`cuda::detect_cuda_version_via_libcuda`] as an example.

pub mod cuda;
pub mod libc;
pub mod linux;
pub mod osx;

use once_cell::sync::OnceCell;
use rattler_conda_types::{GenericVirtualPackage, Platform, Version};
use std::str::FromStr;

use crate::osx::ParseOsxVersionError;
use libc::DetectLibCError;
use linux::ParseLinuxVersionError;
use serde::Deserialize;

/// An enum that represents all virtual package types provided by this library.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum VirtualPackage {
    /// Available on windows
    Win,

    /// Available on unix based platforms
    Unix,

    /// Available when running on Linux
    Linux(Linux),

    /// Available when running on OSX
    Osx(Osx),

    /// Available LibC family and version
    LibC(LibC),

    /// Available Cuda version
    Cuda(Cuda),

    /// The CPU architecture
    Archspec(Archspec),
}

impl From<VirtualPackage> for GenericVirtualPackage {
    fn from(package: VirtualPackage) -> Self {
        match package {
            VirtualPackage::Win => GenericVirtualPackage {
                name: "__win".into(),
                version: Version::from_str("0").unwrap(),
                build_string: "0".into(),
            },
            VirtualPackage::Unix => GenericVirtualPackage {
                name: "__unix".into(),
                version: Version::from_str("0").unwrap(),
                build_string: "0".into(),
            },
            VirtualPackage::Linux(linux) => linux.into(),
            VirtualPackage::Osx(osx) => osx.into(),
            VirtualPackage::LibC(libc) => libc.into(),
            VirtualPackage::Cuda(cuda) => cuda.into(),
            VirtualPackage::Archspec(spec) => spec.into(),
        }
    }
}

impl VirtualPackage {
    /// Returns virtual packages detected for the current system or an error if the versions could
    /// not be properly detected.
    pub fn current() -> Result<&'static [Self], DetectVirtualPackageError> {
        static DETECED_VIRTUAL_PACKAGES: OnceCell<Vec<VirtualPackage>> = OnceCell::new();
        DETECED_VIRTUAL_PACKAGES
            .get_or_try_init(try_detect_virtual_packages)
            .map(Vec::as_slice)
    }
}

/// An error that might be returned by [`VirtualPackage::current`].
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum DetectVirtualPackageError {
    #[error(transparent)]
    ParseLinuxVersion(#[from] ParseLinuxVersionError),

    #[error(transparent)]
    ParseMacOsVersion(#[from] ParseOsxVersionError),

    #[error(transparent)]
    DetectLibC(#[from] DetectLibCError),
}

// Detect the available virtual packages on the system
fn try_detect_virtual_packages() -> Result<Vec<VirtualPackage>, DetectVirtualPackageError> {
    let mut result = Vec::new();
    let platform = Platform::current();

    if platform.is_unix() {
        result.push(VirtualPackage::Unix);
    }

    if platform.is_windows() {
        result.push(VirtualPackage::Win);
    }

    if platform.is_linux() {
        if let Some(linux_version) = Linux::current()? {
            result.push(linux_version.into())
        }
        if let Some(libc) = LibC::current()? {
            result.push(libc.into())
        }
    }

    if platform.is_osx() {
        if let Some(osx) = Osx::current()? {
            result.push(osx.into());
        }
    }

    if let Some(cuda) = Cuda::current() {
        result.push(cuda.into())
    }

    if let Some(archspec) = Archspec::from_platform(platform) {
        result.push(archspec.into())
    }

    Ok(result)
}

/// Linux virtual package description
#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize)]
pub struct Linux {
    /// The version of linux
    /// #[serde(deserialize_with = "from_str")]
    pub version: Version,
}

impl Linux {
    /// Returns the Linux version of the current platform.
    ///
    /// Returns an error if determining the Linux version resulted in an error. Returns `None` if
    /// the current platform is not a Linux based platform.
    pub fn current() -> Result<Option<Self>, ParseLinuxVersionError> {
        Ok(linux::linux_version()?.map(|version| Self { version }))
    }
}

impl From<Linux> for GenericVirtualPackage {
    fn from(linux: Linux) -> Self {
        GenericVirtualPackage {
            name: "__linux".into(),
            version: linux.version,
            build_string: "0".into(),
        }
    }
}

impl From<Linux> for VirtualPackage {
    fn from(linux: Linux) -> Self {
        VirtualPackage::Linux(linux)
    }
}

/// LibC virtual package description
#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize)]
pub struct LibC {
    /// The family of LibC. This could be glibc for instance.
    pub family: String,

    /// The version of the libc distribution.
    /// #[serde(deserialize_with = "from_str")]
    pub version: Version,
}

impl LibC {
    /// Returns the LibC family and version of the current platform.
    ///
    /// Returns an error if determining the LibC family and version resulted in an error. Returns
    /// `None` if the current platform does not have an available version of LibC.
    pub fn current() -> Result<Option<Self>, DetectLibCError> {
        Ok(libc::libc_family_and_version()?.map(|(family, version)| Self { family, version }))
    }
}

impl From<LibC> for GenericVirtualPackage {
    fn from(libc: LibC) -> Self {
        GenericVirtualPackage {
            name: format!("__{}", libc.family.to_lowercase()),
            version: libc.version,
            build_string: "0".into(),
        }
    }
}

impl From<LibC> for VirtualPackage {
    fn from(libc: LibC) -> Self {
        VirtualPackage::LibC(libc)
    }
}

/// Cuda virtual package description
#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize)]
pub struct Cuda {
    /// The maximum supported Cuda version.
    /// #[serde(deserialize_with = "from_str")]
    pub version: Version,
}

impl Cuda {
    /// Returns the maximum Cuda version available on the current platform.
    pub fn current() -> Option<Self> {
        cuda::cuda_version().map(|version| Self { version })
    }
}

impl From<Cuda> for GenericVirtualPackage {
    fn from(cuda: Cuda) -> Self {
        GenericVirtualPackage {
            name: "__cuda".into(),
            version: cuda.version,
            build_string: "0".into(),
        }
    }
}

impl From<Cuda> for VirtualPackage {
    fn from(cuda: Cuda) -> Self {
        VirtualPackage::Cuda(cuda)
    }
}

/// Archspec describes the CPU architecture
#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize)]
pub struct Archspec {
    /// A specification of the architecture family. This could be `x86_64` but it could also include
    /// the full CPU family.
    pub spec: String,
}

impl Archspec {
    /// Returns the current CPU architecture
    pub fn current() -> Option<Self> {
        Self::from_platform(Platform::current())
    }

    /// Returns the CPU architecture for the given platform
    pub fn from_platform(platform: Platform) -> Option<Self> {
        let archspec = match platform {
            Platform::NoArch => return None,
            Platform::Emscripten32 | Platform::Win32 | Platform::Linux32 => "x86",
            Platform::Win64 | Platform::Osx64 | Platform::Linux64 => "x86_64",
            Platform::LinuxAarch64 => "aarch64",
            Platform::LinuxArmV6l => "armv6l",
            Platform::LinuxArmV7l => "armv7l",
            Platform::LinuxPpc64le => "ppc64le",
            Platform::LinuxPpc64 => "ppc64",
            Platform::LinuxS390X => "s390x",
            Platform::LinuxRiscv32 => "riscv32",
            Platform::LinuxRiscv64 => "riscv64",
            Platform::OsxArm64 => "arm64",
            Platform::WinArm64 => "arm64",
        };

        Some(Self {
            spec: archspec.into(),
        })
    }
}

impl From<Archspec> for GenericVirtualPackage {
    fn from(archspec: Archspec) -> Self {
        GenericVirtualPackage {
            name: "__archspec".into(),
            version: Version::from_str("1").unwrap(),
            build_string: archspec.spec,
        }
    }
}

impl From<Archspec> for VirtualPackage {
    fn from(archspec: Archspec) -> Self {
        VirtualPackage::Archspec(archspec)
    }
}

/// OSX virtual package description
#[derive(Clone, Eq, PartialEq, Hash, Debug, Deserialize)]
pub struct Osx {
    /// The OSX version
    /// #[serde(deserialize_with = "from_str")]
    pub version: Version,
}

impl Osx {
    /// Returns the OSX version of the current platform.
    ///
    /// Returns an error if determining the OSX version resulted in an error. Returns `None` if
    /// the current platform is not an OSX based platform.
    pub fn current() -> Result<Option<Self>, ParseOsxVersionError> {
        Ok(osx::osx_version()?.map(|version| Self { version }))
    }
}

impl From<Osx> for GenericVirtualPackage {
    fn from(osx: Osx) -> Self {
        GenericVirtualPackage {
            name: "__osx".into(),
            version: osx.version,
            build_string: "0".into(),
        }
    }
}

impl From<Osx> for VirtualPackage {
    fn from(osx: Osx) -> Self {
        VirtualPackage::Osx(osx)
    }
}

#[cfg(test)]
mod test {
    use crate::VirtualPackage;

    #[test]
    fn doesnt_crash() {
        let virtual_packages = VirtualPackage::current().unwrap();
        println!("{:?}", virtual_packages);
    }
}
