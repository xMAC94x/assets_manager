use std::{
    borrow::Cow,
    collections::HashMap,
    io,
};

use super::Source;


/// The raw representation of embedded files. The common way to create one is the
/// [`embed!`](`super::embed`) macro, and it is used to create an
/// [`Embedded`](`super::Embedded`) source.
///
/// Most of the time you will want a `'static` one, but it can also borrow data
/// from the current context;
///
/// Unlike `Embedded`, it is possible to create it in a const context.
#[cfg_attr(docsrs, doc(cfg(feature = "embedded")))]
#[derive(Clone, Copy, Debug)]
pub struct RawEmbedded<'a> {
    /// A list of files, represented by their id and their extension, with
    /// their content.
    pub files: &'a [((&'a str, &'a str), &'a [u8])],

    /// A list of directory, represented by their id, with the list of files
    /// they contain.
    pub dirs: &'a [(&'a str, &'a [(&'a str, &'a str)])],
}

/// A [`Source`] which is embedded in the binary. It is created using a
/// [`RawEmbedded`] struct.
///
/// ## Pros and Cons
///
/// Embedding assets enables to easily share a program as a single binary, which
/// is especially useful for WebAssembly, where no file system is available.
/// Moreover, you might experience performance gain, as no I/O is necessary to
/// load an asset.
///
/// However, embedding assets comes with a great cost. It can really slow
/// development speed, because it significantly increases compile time and it
/// makes it hard to edit external files (you have to recompile the program
/// each time you edit an asset). Hot-reloading is of course impossible. For
/// these reasons, you should only use this source for release builds. It also
/// tends to creates large binarie, which increases memory usage.
///
/// ## Usage
///
/// ```no_run
/// use assets_manager::{AssetCache, source::{embed, Embedded}};
///
/// let embed = Embedded::from(embed!("assets"));
/// let cache = AssetCache::with_source(embed);
/// ```
#[cfg_attr(docsrs, doc(cfg(feature = "embedded")))]
#[derive(Clone, Debug)]
pub struct Embedded<'a> {
    files: HashMap<(&'a str, &'a str), &'a [u8]>,
    dirs: HashMap<&'a str, &'a [(&'a str, &'a str)]>,
}

impl<'a> From<RawEmbedded<'a>> for Embedded<'a> {
    fn from(raw: RawEmbedded<'a>) -> Embedded<'a> {
        Embedded {
            files: raw.files.iter().copied().collect(),
            dirs: raw.dirs.iter().copied().collect(),
        }
    }
}

impl<'a> Source for Embedded<'a> {
    fn read(&self, id: &str, ext: &str) -> io::Result<Cow<[u8]>> {
        match self.files.get(&(id, ext)) {
            Some(content) => Ok(Cow::Borrowed(content)),
            None => Err(io::ErrorKind::NotFound.into()),
        }
    }

    fn read_dir(&self, id: &str, ext: &[&str]) -> io::Result<Vec<String>> {
        let dir = self.dirs.get(id).ok_or(io::ErrorKind::NotFound)?;

        Ok(dir.iter().copied()
            .filter(|(_, file_ext)| ext.contains(file_ext))
            .map(|(id,_)| id.to_owned())
            .collect()
        )
    }
}
