#[cfg(feature = "hot-reloading")]
use crate::{
    Asset, Compound,
    hot_reloading::{
        AssetReloadInfos,
        CompoundReloadInfos,
        HotReloader,
        UpdateMessage,
    },
    utils::PrivateMarker,
};

#[cfg(doc)]
use crate::AssetCache;

use std::{
    borrow::Cow,
    fmt,
    fs,
    io,
    path::{Path, PathBuf},
};

use super::Source;


#[inline]
pub fn extension_of(path: &Path) -> Option<&str> {
    match path.extension() {
        Some(ext) => ext.to_str(),
        None => Some(""),
    }
}

#[inline]
fn has_extension(path: &Path, ext: &[&str]) -> bool {
    match extension_of(path) {
        Some(file_ext) => ext.contains(&file_ext),
        None => false,
    }
}

/// A [`Source`] to load assets from a directory in the file system.
///
/// This is the default `Source` of [`AssetCache`].
///
/// ## Hot-reloading
///
/// This source supports hot-reloading: when a file is edited, the corresponding
/// assets are reloaded when [`AssetCache::hot_reload`] is called.
///
/// ## WebAssembly
///
/// This source does not work in WebAssembly, because there is no file system.
/// When called, it always returns an error.
pub struct FileSystem {
    path: PathBuf,

    #[cfg(feature = "hot-reloading")]
    pub(crate) reloader: Option<HotReloader>,
}

impl FileSystem {
    /// Creates a new `FileSystem` from a directory.
    ///
    /// Generally you do not need to call this function directly, as the
    /// [`AssetCache::new`] method provides a shortcut to create a cache
    /// reading from the filesystem.
    ///
    /// # Errors
    ///
    /// An error can occur if `path` is not a valid readable directory.
    ///
    /// If hot-reloading fails to start (if feature `hot-reloading` is used),
    /// an error is logged and this function returns `Ok`.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<FileSystem> {
        Self::_new(path.as_ref(), true)
    }

    /// Same as `new`, but does not start hot-reloading.
    ///
    /// If feature `hot-reloading` is not enabled, this function is equivalent
    /// to `new`.
    pub fn without_hot_reloading<P: AsRef<Path>>(path: P) -> io::Result<FileSystem> {
        Self::_new(path.as_ref(), false)
    }

    fn _new(path: &Path, _hot_reloading: bool) -> io::Result<FileSystem> {
        let path = path.canonicalize()?;
        let _ = path.read_dir()?;

        #[cfg(feature = "hot-reloading")]
        let reloader = if _hot_reloading {
            match HotReloader::start(&path) {
                Ok(r) => Some(r),
                Err(err) => {
                    log::error!("Unable to start hot-reloading: {}", err);
                    None
                }
            }
        } else {
            None
        };

        Ok(FileSystem {
            path,

            #[cfg(feature = "hot-reloading")]
            reloader,
        })
    }

    /// Gets the path of the source's root.
    ///
    /// The path is currently given as absolute, but this may change in the future.
    #[inline]
    pub fn root(&self) -> &Path {
        &self.path
    }

    /// Returns the path of the (eventual) file represented by an id and an
    /// extension.
    pub fn path_of(&self, id: &str, ext: &str) -> PathBuf {
        let mut path = self.path.clone();
        path.extend(id.split('.'));
        path.set_extension(ext);
        path
    }
}

impl Source for FileSystem {
    fn read(&self, id: &str, ext: &str) -> io::Result<Cow<[u8]>> {
        let path = self.path_of(id, ext);
        fs::read(path).map(Into::into)
    }

    fn read_dir(&self, id: &str, ext: &[&str]) -> io::Result<Vec<String>> {
        let dir_path = self.path_of(id, "");
        let entries = fs::read_dir(dir_path)?;

        let mut loaded = Vec::new();

        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                if !has_extension(&path, ext) {
                    continue;
                }

                let name = match path.file_stem().and_then(|n| n.to_str()) {
                    Some(name) => name,
                    None => continue,
                };

                if path.is_file() {
                    loaded.push(name.into());
                }
            }
        }

        Ok(loaded)
    }

    #[cfg(feature = "hot-reloading")]
    fn _add_asset<A: Asset, P: PrivateMarker>(&self, id: &str) {
        if let Some(reloader) = &self.reloader {
            for ext in A::EXTENSIONS {
                let path = self.path_of(id, ext);
                let msg = UpdateMessage::AddAsset(AssetReloadInfos::of::<A>(path, id.into()));
                reloader.send_update(msg);
            }
        }
    }

    #[cfg(feature = "hot-reloading")]
    fn _add_dir<A: Asset, P: PrivateMarker>(&self, id: &str) {
        if let Some(reloader) = &self.reloader {
            let path = self.path_of(id, "");
            let msg = UpdateMessage::AddDir(AssetReloadInfos::of::<A>(path, id.into()), A::EXTENSIONS);
            reloader.send_update(msg);
        }
    }

    #[cfg(feature = "hot-reloading")]
    fn _clear<P: PrivateMarker>(&mut self) {
        if let Some(reloader) = &self.reloader {
            reloader.send_update(UpdateMessage::Clear);
        }
    }

    #[cfg(feature = "hot-reloading")]
    fn _add_compound<A: Compound, P: PrivateMarker>(&self, id: &str, deps: crate::utils::DepsRecord) {
        if let Some(reloader) = &self.reloader {
            reloader.send_update(UpdateMessage::AddCompound(CompoundReloadInfos::of::<A>(id.into(), deps.0)))
        }
    }

    #[cfg(feature = "hot-reloading")]
    #[doc(hidden)]
    fn _support_hot_reloading<P: PrivateMarker>(&self) -> bool {
        self.reloader.is_some()
    }
}

impl fmt::Debug for FileSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileSystem").field("root", &self.path).finish()
    }
}
