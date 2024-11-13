use std::{io::Read, path::Path};

pub trait OwnersOfPath {
    type Owner;

    /// Resolve a list of owners matching a given path
    fn of<P>(&self, path: P) -> Option<Vec<Self::Owner>>
    where
        P: AsRef<Path>;
}

pub trait FromPath: Sized {
    /// Parse a CODEOWNERS file existing at a given path
    fn from_path<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>;
}

pub trait FromReader: Sized {
    /// Parse a CODEOWNERS file from some readable source
    fn from_reader<R>(read: R) -> anyhow::Result<Self>
    where
        R: Read;
}
