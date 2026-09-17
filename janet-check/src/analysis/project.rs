//! What `project.janet` sees beyond the root environment: the bindings jpm and janet-pm
//! (`spork/declare-cc`) evaluate it with. The installed tools are dumped with their docstrings
//! and source maps (`janet/project.janet`); this table documents the keys of `declare-*`, which
//! their docstrings leave out, and stands in for the rest when neither tool is installed.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::stdlib::{CoreBinding, CoreKind};

/// Name, parameters and doc. Keys are written as `&named` parameters, so signature help follows
/// them; they are the union of jpm's and janet-pm's, marked where only one tool reads them.
const VOCABULARY: &[(&str, &str, &str)] = &[
    (
        "declare-project",
        "&named name description author license url repo version dependencies",
        "Declares the project and its standard tasks: `build`, `clean` and `install`, with \
         `test` under jpm and `check` under janet-pm. Must come first in `project.janet`.\n\n\
         - `:name`: the package name, which names its install manifest.\n\
         - `:description`, `:author`, `:license`, `:url`, `:repo`: metadata.\n\
         - `:version`: recorded in the manifest.\n\
         - `:dependencies`: what `jpm deps` or `janet-pm deps` installs. Each is a git URL, a \
         name from the package index (`\"spork\"`), or `{:url … :tag …}` to pin a revision.",
    ),
    (
        "declare-source",
        "&named source prefix",
        "Registers Janet modules for installation; nothing is built.\n\n\
         - `:source`: a file or directory, or an array of them, copied into the module path \
         (`JANET_PATH`).\n\
         - `:prefix`: a directory under the module path to install into, so that \
         `(import prefix/module)` finds them.",
    ),
    (
        "declare-native",
        "&named name source embedded headers deps defines cflags cppflags c++flags lflags ldflags \
         libs static-libs dynamic-libs smart-libs pkg-config-libs pkg-config-flags msvc-libs \
         c-std c++-std target-os use-rpath use-rdynamic nostatic native-deps optimize",
        "Builds a native module: a shared library to `import`, and a static one to link into \
         executables. Returns the built paths as `@{:native … :static …}`.\n\n\
         - `:name`: the module name, `\"mylib\"` or `\"prefix/mylib\"`.\n\
         - `:source`: `.c`, `.cc` and `.cpp` files; under jpm also `.janet` files that print C \
         code.\n\
         - `:embedded`: Janet files compiled into the module as byte buffers.\n\
         - `:headers`, `:deps` (janet-pm): files every object depends on.\n\
         - `:defines`: `{\"NAME\" \"value\"}`; `true` defines `NAME` without a value.\n\
         - `:cflags`: C compiler flags; `:cppflags` (jpm) or `:c++flags` (janet-pm) for C++.\n\
         - `:lflags`: linker flags. `:ldflags`: libraries to link, an alias for `:libs` under \
         janet-pm.\n\
         - `:libs`, `:static-libs`, `:dynamic-libs`, `:msvc-libs` (janet-pm): libraries to link; \
         `:smart-libs` picks static or dynamic per library.\n\
         - `:pkg-config-libs` (janet-pm): packages whose flags `pkg-config` adds, with \
         `:pkg-config-flags` passed to it.\n\
         - `:c-std`, `:c++-std` (janet-pm): the standard as a number, 99 and 11 by default.\n\
         - `:target-os`, `:use-rpath`, `:use-rdynamic` (janet-pm): cross builds and linking.\n\
         - `:nostatic` (janet-pm): skip the static library.\n\
         - `:native-deps` (jpm): native modules to link against. `:optimize` (jpm): 0 to 3.",
    ),
    (
        "declare-executable",
        "&named name entry install headers deps defines cflags c++flags lflags ldflags libs \
         static-libs dynamic-libs smart-libs pkg-config-libs pkg-config-flags msvc-libs c-std \
         c++-std target-os use-rpath use-rdynamic static no-compile no-core",
        "Builds a standalone executable into the build directory (`build/` under jpm, \
         `_build/<type>/` under janet-pm). The entry file is evaluated at build time and its \
         `main` function is marshalled into the binary.\n\n\
         - `:name`: the executable name; `.exe` is added on Windows.\n\
         - `:entry`: the Janet file that defines `main`.\n\
         - `:install`: also install it into the binary path.\n\
         - `:headers`, `:deps`: files the build depends on.\n\
         - `:cflags`, `:lflags`, `:ldflags`: compiler flags, linker flags, libraries.\n\
         - `:static` (janet-pm): link fully statically; not on macOS.\n\
         - `:no-compile`: only generate the C source, `<name>.c`.\n\
         - `:no-core`: leave the core library out of the image.\n\
         - The other keys (janet-pm) are those of `declare-native`.",
    ),
    (
        "declare-binscript",
        "&named main name hardcode-syspath is-janet",
        "Installs a script into the binary path, with a `.bat` shim on Windows.\n\n\
         - `:main`: the script.\n\
         - `:name` (janet-pm): the installed name, the script's file name by default.\n\
         - `:hardcode-syspath`: insert a line that sets `:syspath` to the install's module path, \
         so the script finds its modules whatever `JANET_PATH` is. Under janet-pm `:dynamic` \
         restores the original syspath at the end of the script.\n\
         - `:is-janet`: a Janet script, which gets a shebang for `janet`.",
    ),
    (
        "declare-archive",
        "&named name entry deps",
        "Builds `<name>.jimage`, an image of the entry module and everything it requires, and \
         installs it into the module path.\n\n\
         - `:name`: the image name.\n\
         - `:entry`: the module to `require`, as an import path.\n\
         - `:deps`: files the image depends on.",
    ),
    (
        "declare-headers",
        "&named headers prefix",
        "Installs C headers for other native modules to include.\n\n\
         - `:headers`: a file or an array of them.\n\
         - `:prefix`: a directory under the module path to install into.",
    ),
    (
        "declare-bin",
        "&named main name",
        "Installs a file into the binary path as is; janet-pm treats a `.janet` file as \
         `declare-binscript`.\n\n\
         - `:main`: the file.\n\
         - `:name` (janet-pm): the installed name.",
    ),
    (
        "declare-manpage",
        "page",
        "Installs the manual page `page`: into `(dyn :manpath)` under jpm, nothing when it is \
         not set; into `<syspath>/man/man1` under janet-pm.",
    ),
    (
        "declare-documentation",
        "&named source prefix",
        "Installs the file `source` into `<syspath>/man/<prefix>/`. janet-pm only.",
    ),
    (
        "install-rule",
        "src destdir",
        "Adds copying `src` into the directory `destdir` to the `install` task and records it in \
         the manifest, so `uninstall` removes it. Release builds only.",
    ),
    (
        "install-file-rule",
        "src dest",
        "Adds copying the file `src` to the path `dest` to the `install` task and records it in \
         the manifest.",
    ),
    (
        "uninstall",
        "name",
        "Removes every file the manifest of the package `name` lists, then the manifest.",
    ),
    (
        "run-tests",
        "&opt root-directory",
        "Runs every `.janet` file under `root-directory` (`test` when omitted) as a script. Exits \
         with 1 when any script exits non-zero.",
    ),
    (
        "rule",
        "target deps & body",
        "Adds a rule: `body` produces the file `target` after the rules in `deps` run, whenever \
         `target` is out of date. `target` may be an array of outputs; the first names the rule.",
    ),
    (
        "task",
        "target deps & body",
        "Adds a task: `body` runs every time `target` is invoked, after the rules in `deps`. \
         Tasks with the same name add up, so `(task \"build\" …)` extends the standard build.",
    ),
    ("phony", "target deps & body", "Alias for `task`."),
    (
        "sh-rule",
        "target deps & body",
        "A `rule` whose recipe is a shell command made of `body`.",
    ),
    (
        "sh-task",
        "target deps & body",
        "A `task` whose recipe is a shell command made of `body`.",
    ),
    ("sh-phony", "target deps & body", "Alias for `sh-task`."),
    (
        "add-dep",
        "target dep",
        "Makes the rule `target` depend on `dep`, a rule or a file: \
         `(add-dep \"build\" \"build/extra.so\")`.",
    ),
    ("add-input", "target input", "Alias for `add-dep`."),
    (
        "add-output",
        "target output",
        "Adds an output file to the rule `target`, which is still referred to by its first one.",
    ),
    (
        "add-body",
        "target & body",
        "Appends `body` to the recipe of the existing rule `target`, leaving its dependencies as \
         they are.",
    ),
    (
        "post-deps",
        "& body",
        "Evaluates `body` only when dependencies are available. `project.janet` is also loaded \
         before they are installed, so code that imports a dependency goes here.",
    ),
    (
        "shell",
        "& args",
        "Runs a command with `JANET_PATH` set to the module path; raises an error when it exits \
         non-zero.",
    ),
    (
        "exec-slurp",
        "& args",
        "Runs a command and returns its stdout with trailing whitespace trimmed.",
    ),
    (
        "copy",
        "src dest",
        "Copies a file or a directory recursively.",
    ),
    (
        "rm",
        "path",
        "Removes a file, or a directory with everything in it.",
    ),
    (
        "create-dirs",
        "dest",
        "Creates the missing parent directories of the file path `dest`, like `mkdir -p`.",
    ),
];

/// Macros among [`VOCABULARY`], for when no tool says so.
const MACROS: [&str; 8] = [
    "rule",
    "task",
    "phony",
    "sh-rule",
    "sh-task",
    "sh-phony",
    "add-body",
    "post-deps",
];

/// Values rather than functions: name and doc.
const VALUES: [(&str, &str); 4] = [
    (
        "default-cflags",
        "The C compiler flags jpm is configured with; an empty array under janet-pm.",
    ),
    (
        "default-cppflags",
        "The C++ compiler flags jpm is configured with; an empty array under janet-pm.",
    ),
    (
        "default-lflags",
        "The linker flags jpm is configured with; an empty array under janet-pm.",
    ),
    (
        "default-ldflags",
        "The libraries jpm links with; an empty array under janet-pm.",
    ),
];

/// Every name this table knows.
pub fn names() -> impl Iterator<Item = &'static str> {
    let calls = VOCABULARY.iter().map(|(name, ..)| *name);
    calls.chain(VALUES.iter().map(|(name, _)| *name))
}

/// The bindings `dumped` from the installed tools with this table's docs: over theirs for
/// `declare-*`, which leave the keys out, and for the names neither tool has.
pub fn vocabulary(
    dumped: impl Iterator<Item = (String, CoreBinding)>,
) -> HashMap<String, CoreBinding> {
    documented().fold(dumped.collect(), |mut vocabulary, (name, kind, doc)| {
        match vocabulary.entry(name.to_string()) {
            Entry::Occupied(mut entry) if name.starts_with("declare-") => {
                entry.get_mut().doc = Some(doc);
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(entry) => {
                entry.insert(CoreBinding {
                    kind,
                    doc: Some(doc),
                    location: None,
                });
            }
        }
        vocabulary
    })
}

fn documented() -> impl Iterator<Item = (&'static str, CoreKind, String)> {
    let calls = VOCABULARY.iter().map(|(name, params, doc)| {
        let kind = if MACROS.contains(name) {
            CoreKind::Macro
        } else {
            CoreKind::Function
        };
        let call = [*name, params].join(" ");
        (*name, kind, format!("({})\n\n{doc}", call.trim_end()))
    });
    let values = VALUES
        .iter()
        .map(|(name, doc)| (*name, CoreKind::Value, (*doc).to_string()));
    calls.chain(values)
}

#[cfg(test)]
mod tests;
