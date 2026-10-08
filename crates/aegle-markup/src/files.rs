//! Multi-file programs: `use` resolution, import cycles and rendered diagnostics.

use std::fmt;

use crate::{Document, ElementSpec, Error, Item, Node, Program, check_program, parse};

/// Most files one program may load, bounding import work.
const MAX_FILES: usize = 256;

/// One loaded source file of a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// Normalized `/`-separated path, as passed to the reader.
    pub path: String,
    /// UTF-8 source text.
    pub source: String,
}

/// A program diagnostic rendered as `file:line:column: message`, followed by
/// the source line and a caret, or a read failure naming the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramError(pub String);

impl fmt::Display for ProgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProgramError {}

/// The parsed files of a program, entry first, before checking.
#[derive(Debug)]
pub struct Sources {
    /// The files, entry first.
    pub files: Vec<File>,
    documents: Vec<Document>,
}

impl Sources {
    /// Loads `entry` and every file it imports with `use`.
    ///
    /// `read` receives normalized paths: the entry as given, and each import
    /// joined to its importer's directory with `.` and `..` resolved. Import
    /// paths must be relative and `/`-separated. Import cycles are errors; a
    /// file imported twice is loaded once. Files use the default parsing
    /// limits, and at most 256 files are loaded.
    pub fn load(
        entry: &str,
        read: &mut dyn FnMut(&str) -> Result<String, String>,
    ) -> Result<Self, ProgramError> {
        let mut loader = Loader {
            files: Vec::new(),
            documents: Vec::new(),
            active: Vec::new(),
            read,
        };
        loader.load(entry.to_owned())?;
        Ok(Self {
            files: loader.files,
            documents: loader.documents,
        })
    }

    /// Whether the program is one static document ([`Document::is_static`]).
    pub fn is_static(&self) -> bool {
        self.documents.len() == 1 && self.documents[0].is_static()
    }

    /// The element names the files write, in first-use order: every node
    /// name other than `Window` and the components they declare.
    pub fn elements(&self) -> Vec<String> {
        fn node(node: &Node, components: &[&str], names: &mut Vec<String>) {
            let name = node.name.as_str();
            if name != "Window" && !components.contains(&name) && !names.iter().any(|n| n == name) {
                names.push(node.name.clone());
            }
            visit(&node.children, components, names);
        }
        fn visit(items: &[Item], components: &[&str], names: &mut Vec<String>) {
            for item in items {
                match item {
                    Item::Node(child) => node(child, components, names),
                    Item::If(_, then, otherwise) => {
                        visit(then, components, names);
                        visit(otherwise, components, names);
                    }
                    Item::For(.., body) => visit(body, components, names),
                    Item::Slot => {}
                }
            }
        }
        let documents = &self.documents;
        let components: Vec<&str> = documents
            .iter()
            .flat_map(|document| document.components.iter().map(|c| c.name.as_str()))
            .collect();
        let mut names = Vec::new();
        for document in documents {
            let roots = document
                .root
                .iter()
                .chain(document.components.iter().map(|c| &c.root));
            for root in roots {
                node(root, &components, &mut names);
            }
        }
        names
    }

    /// Checks the program against `specs`; returns it with its files.
    pub fn check(self, specs: &[ElementSpec<'_>]) -> Result<(Program, Vec<File>), ProgramError> {
        let files = self.files;
        let mut program = check_program(self.documents, specs)
            .map_err(|(file, error)| render(&files[file], &error))?;
        program.files = files.iter().map(|file| file.path.clone()).collect();
        Ok((program, files))
    }
}

/// Loads `entry` and its imports with [`Sources::load`], then checks the
/// program against `specs`. Returns the program and its files, entry first.
pub fn compile(
    entry: &str,
    specs: &[ElementSpec<'_>],
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<(Program, Vec<File>), ProgramError> {
    Sources::load(entry, read)?.check(specs)
}

struct Loader<'r> {
    files: Vec<File>,
    documents: Vec<Document>,
    /// Paths of the files whose imports are being loaded.
    active: Vec<String>,
    read: &'r mut dyn FnMut(&str) -> Result<String, String>,
}

impl Loader<'_> {
    fn load(&mut self, path: String) -> Result<(), ProgramError> {
        if self.files.len() == MAX_FILES {
            return Err(ProgramError(format!(
                "{path}: a program loads at most 256 files"
            )));
        }
        let source =
            (self.read)(&path).map_err(|error| ProgramError(format!("{path}: {error}")))?;
        let file = File { path, source };
        let document = parse(&file.source).map_err(|error| render(&file, &error))?;
        let uses = document.uses.clone();
        let index = self.files.len();
        self.active.push(file.path.clone());
        self.files.push(file);
        self.documents.push(document);
        for import in uses {
            let error =
                |message: &str| render(&self.files[index], &Error::new(import.span, message));
            if import.path.starts_with('/') || import.path.contains(['\\', ':']) {
                return Err(error("use paths must be relative and '/'-separated"));
            }
            let target = join(&self.files[index].path, &import.path);
            if self.active.contains(&target) {
                return Err(error("import cycle"));
            }
            if !self.files.iter().any(|file| file.path == target) {
                self.load(target)?;
            }
        }
        self.active.pop();
        Ok(())
    }
}

/// Joins `path` to the directory of `importer`, resolving `.` and `..`.
fn join(importer: &str, path: &str) -> String {
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

fn render(file: &File, error: &Error) -> ProgramError {
    ProgramError(error.render(&file.source, &file.path))
}
