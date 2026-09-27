use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use regex::Regex;
use tree_sitter::{Node, Parser};

use crate::core::adapter::{
    AdapterError, BuildResult, Diagnostic, LanguageAdapter, RunSpec, Severity,
};
use crate::core::model::{
    module_model, ClassKind, ClassModel, EnumConstantModel, FieldModel, FunctionModel, MethodModel,
    Multiplicity, ParameterModel, Visibility,
};
use crate::core::project::Project;

const EXTENSIONS: &[&str] = &["kt"];
const CLASSES_DIR: &str = ".idecode/classes";

const TYPE_DECLARATIONS: &[&str] = &["class_declaration", "object_declaration"];

const TYPE_NODE_KINDS: &[&str] = &["user_type", "nullable_type", "function_type"];

const COLLECTION_TYPES: &[&str] = &[
    "Array",
    "Collection",
    "Iterable",
    "List",
    "Map",
    "MutableCollection",
    "MutableIterable",
    "MutableList",
    "MutableMap",
    "MutableSet",
    "Set",
];

const BUILTIN_TYPES: &[&str] = &[
    "Any",
    "Boolean",
    "BooleanArray",
    "Byte",
    "ByteArray",
    "Char",
    "CharArray",
    "Comparable",
    "Double",
    "DoubleArray",
    "Float",
    "FloatArray",
    "Int",
    "IntArray",
    "Long",
    "LongArray",
    "Nothing",
    "Short",
    "ShortArray",
    "String",
    "Unit",
];

pub struct KotlinAdapter;

impl KotlinAdapter {
    pub fn new() -> Self {
        KotlinAdapter
    }
}

impl Default for KotlinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAdapter for KotlinAdapter {
    fn id(&self) -> &'static str {
        "kotlin"
    }

    fn display_name(&self) -> &'static str {
        "Kotlin"
    }

    fn extensions(&self) -> &'static [&'static str] {
        EXTENSIONS
    }

    fn entry_label(&self) -> &'static str {
        "Archivo principal"
    }

    fn compile(&self, project: &Project) -> Result<BuildResult, AdapterError> {
        if project.sources.is_empty() {
            return Ok(BuildResult {
                success: false,
                diagnostics: vec![Diagnostic {
                    severity: Severity::Error,
                    message: "No hay archivos .kt en el proyecto.".into(),
                    file: None,
                    line: None,
                    column: None,
                }],
            });
        }

        let classes_dir = project.root.join(CLASSES_DIR);
        let _ = fs::remove_dir_all(&classes_dir);
        fs::create_dir_all(&classes_dir)?;

        let kotlinc = kotlin_tool("kotlinc");
        let mut command = Command::new(&kotlinc);
        crate::runner::sanitize_child_env(&mut command);
        command
            .current_dir(&project.root)
            .arg("-d")
            .arg(&classes_dir);
        for source in &project.sources {
            command.arg(&source.path);
        }

        let output = command
            .output()
            .map_err(|error| missing_tool(error, &kotlinc))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

        let mut diagnostics = parse_kotlinc(&stderr, &project.root);
        if diagnostics.is_empty() {
            diagnostics = parse_kotlinc(&stdout, &project.root);
        }
        if !output.status.success() && diagnostics.is_empty() {
            if toolchain_crash(&stderr) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Note,
                    message: "El compilador de Kotlin ha fallado internamente. Suele significar que tu «kotlinc» es demasiado viejo para tu JDK (por ejemplo, Kotlin 1.3 con JDK 17). Actualiza Kotlin (con SDKMAN: «sdk install kotlin») o instala un JDK compatible.".into(),
                    file: None,
                    line: None,
                    column: None,
                });
            } else {
                let message = if stderr.trim().is_empty() {
                    "La compilación falló.".to_string()
                } else {
                    stderr.trim().to_string()
                };
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    file: None,
                    line: None,
                    column: None,
                });
            }
        }

        Ok(BuildResult {
            success: output.status.success(),
            diagnostics,
        })
    }

    fn run_spec(
        &self,
        project: &Project,
        entry: Option<&str>,
        args: &[String],
    ) -> Result<RunSpec, AdapterError> {
        let classes_dir = project.root.join(CLASSES_DIR);
        if !classes_dir.is_dir() {
            return Err(AdapterError::Invalid(
                "Compila el proyecto antes de ejecutarlo.".into(),
            ));
        }

        let entry = match entry {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => detect_entry(project)?,
        };

        let classes = classes_dir.to_string_lossy().into_owned();
        let (program, mut arguments) = match kotlin_stdlib() {
            Some(stdlib) => {
                let classpath = std::env::join_paths([&classes_dir, &stdlib])
                    .map_err(|_| AdapterError::Invalid("Classpath inválido.".into()))?;
                (
                    java_tool(),
                    vec![
                        "-Dfile.encoding=UTF-8".to_string(),
                        "-Dstdout.encoding=UTF-8".to_string(),
                        "-Dstderr.encoding=UTF-8".to_string(),
                        "-cp".to_string(),
                        classpath.to_string_lossy().into_owned(),
                        entry,
                    ],
                )
            }
            None => {
                let kotlin = kotlin_tool("kotlin");
                (kotlin, vec!["-cp".to_string(), classes, entry])
            }
        };
        arguments.extend(args.iter().cloned());

        Ok(RunSpec {
            program,
            args: arguments,
            cwd: project.root.clone(),
            env: Vec::new(),
        })
    }

    fn analyze(&self, project: &Project) -> Result<Vec<ClassModel>, AdapterError> {
        parse_project(project)
    }

    fn new_source(&self, name: &str) -> String {
        let class_name = Path::new(name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.trim().is_empty())
            .unwrap_or("NuevaClase");
        format!("class {class_name} {{\n}}\n")
    }

    fn validate_new_file(&self, name: &str) -> Result<(), String> {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if is_kotlin_identifier(stem) {
            Ok(())
        } else {
            Err(format!(
                "«{stem}» no es un nombre de clase válido en Kotlin. Empieza por letra o «_» y usa solo letras, dígitos o «_»."
            ))
        }
    }
}

fn is_kotlin_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let starts_ok =
        matches!(characters.next(), Some(first) if first == '_' || first.is_alphabetic());
    starts_ok && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn kotlin_home_dirs() -> Vec<PathBuf> {
    let mut extra_dirs = Vec::new();
    if let Ok(home) = std::env::var("KOTLIN_HOME") {
        if !home.trim().is_empty() {
            let home = Path::new(home.trim());
            extra_dirs.push(home.join("bin"));
            extra_dirs.push(home.to_path_buf());
        }
    }
    extra_dirs
}

fn kotlin_tool(name: &str) -> PathBuf {
    crate::core::tools::find_program(&[name], &kotlin_home_dirs())
        .unwrap_or_else(|| PathBuf::from(name))
}

fn java_tool() -> PathBuf {
    let mut extra_dirs = Vec::new();
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if !home.trim().is_empty() {
            let home = Path::new(home.trim());
            extra_dirs.push(home.join("bin"));
            extra_dirs.push(home.to_path_buf());
        }
    }
    crate::core::tools::find_program(&["java"], &extra_dirs)
        .unwrap_or_else(|| PathBuf::from("java"))
}

fn kotlin_stdlib() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("KOTLIN_HOME") {
        if !home.trim().is_empty() {
            roots.push(PathBuf::from(home.trim()));
        }
    }
    if let Some(kotlinc) = crate::core::tools::find_program(&["kotlinc"], &kotlin_home_dirs()) {
        let resolved = fs::canonicalize(&kotlinc)
            .map(|path| crate::core::project::strip_verbatim(&path))
            .unwrap_or(kotlinc);
        if let Some(home) = resolved.parent().and_then(Path::parent) {
            roots.push(home.to_path_buf());
        }
    }

    for root in roots {
        let lib = root.join("lib");
        let plain = lib.join("kotlin-stdlib.jar");
        if plain.is_file() {
            return Some(plain);
        }
        let Ok(entries) = fs::read_dir(&lib) else {
            continue;
        };
        let mut candidate = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value == "jar")
                    && path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| {
                            value.starts_with("kotlin-stdlib")
                                && !value.contains("-sources")
                                && !value.contains("-javadoc")
                        })
            })
            .min_by_key(|path| {
                path.file_name()
                    .map(|value| value.to_string_lossy().len())
                    .unwrap_or(usize::MAX)
            });
        if candidate.is_some() {
            return candidate.take();
        }
    }
    None
}

fn missing_tool(error: std::io::Error, tool: &Path) -> AdapterError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AdapterError::ToolMissing {
            tool: tool.to_string_lossy().into_owned(),
            adapter: "Kotlin".to_string(),
            hint: "Instala el compilador de Kotlin (por ejemplo con SDKMAN: «sdk install kotlin», o vía snap/brew) y asegúrate de que «kotlinc» esté en el PATH o de definir KOTLIN_HOME. Kotlin necesita además un JDK.".into(),
        }
    } else {
        AdapterError::Io(error.to_string())
    }
}

fn detect_entry(project: &Project) -> Result<String, AdapterError> {
    let classes = parse_project(project)?;
    let mut candidates: Vec<&ClassModel> = classes
        .iter()
        .filter(|model| model.kind == ClassKind::Module)
        .filter(|model| {
            model
                .functions
                .iter()
                .any(|function| function.name == "main")
        })
        .collect();
    if candidates.is_empty() {
        return Err(AdapterError::Invalid(
            "No se encontró «fun main()» en el nivel superior de ningún archivo. Añádela a un archivo .kt e inténtalo de nuevo.".into(),
        ));
    }
    candidates.sort_by(|a, b| a.file.cmp(&b.file));
    let chosen = candidates
        .iter()
        .find(|model| {
            Path::new(&model.file)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.eq_ignore_ascii_case("main"))
        })
        .copied()
        .or_else(|| candidates.first().copied())
        .expect("candidatos no vacíos");

    let stem = Path::new(&chosen.file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Main");
    let facade = facade_class(stem);
    Ok(match chosen.package.as_deref() {
        Some(package) if !package.is_empty() => format!("{package}.{facade}"),
        _ => facade,
    })
}

fn facade_class(stem: &str) -> String {
    let mut characters = stem.chars();
    match characters.next() {
        Some(first) => {
            let mut name = first.to_uppercase().collect::<String>();
            name.push_str(characters.as_str());
            name.push_str("Kt");
            name
        }
        None => "MainKt".to_string(),
    }
}

fn parse_project(project: &Project) -> Result<Vec<ClassModel>, AdapterError> {
    let files: Vec<(&str, &str)> = project
        .sources
        .iter()
        .map(|source| (source.relative.as_str(), source.content.as_str()))
        .collect();
    parse_files(&files)
}

fn parse_files(files: &[(&str, &str)]) -> Result<Vec<ClassModel>, AdapterError> {
    let mut classes = Vec::new();
    for (file, source) in files {
        classes.extend(parse_source(file, source)?);
    }
    resolve_supertypes(&mut classes);
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(classes)
}

fn resolve_supertypes(classes: &mut [ClassModel]) {
    let interfaces: HashSet<String> = classes
        .iter()
        .filter(|class| class.kind == ClassKind::Interface)
        .map(|class| class.simple_name.clone())
        .collect();

    for class in classes.iter_mut() {
        if class.kind != ClassKind::Class
            && class.kind != ClassKind::Record
            && class.kind != ClassKind::Enum
            && class.kind != ClassKind::Interface
        {
            continue;
        }
        let pending = std::mem::take(&mut class.implements);
        if class.kind == ClassKind::Interface {
            class.extends.extend(pending);
            continue;
        }
        for base in pending {
            let simple = base.split('<').next().unwrap_or(&base).trim();
            let simple = simple.rsplit('.').next().unwrap_or(simple).trim();
            if interfaces.contains(simple) {
                class.implements.push(base);
            } else {
                class.extends.push(base);
            }
        }
    }
}

fn parse_source(file: &str, source: &str) -> Result<Vec<ClassModel>, AdapterError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .map_err(|error| AdapterError::Io(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AdapterError::Invalid(format!("No se pudo analizar {file}")))?;
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let package = package_name(root, bytes);

    let mut classes = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&child.kind()) {
            collect_types(child, bytes, file, package.as_deref(), None, &mut classes);
        }
    }

    let module = collect_module(root, bytes, file, package.as_deref());
    if !module.functions.is_empty() || !module.fields.is_empty() {
        classes.push(module);
    }
    Ok(classes)
}

fn package_name(root: Node, source: &[u8]) -> Option<String> {
    let declaration = find_child(root, "package_header")?;
    let name = text(declaration, source);
    let name = name
        .strip_prefix("package")
        .map(str::trim)
        .unwrap_or(&name)
        .to_string();
    (!name.is_empty()).then_some(name)
}

fn collect_types(
    node: Node,
    source: &[u8],
    file: &str,
    package: Option<&str>,
    outer: Option<&str>,
    classes: &mut Vec<ClassModel>,
) {
    let Some(class) = parse_type(node, source, file, package, outer) else {
        return;
    };

    let mut nested = Vec::new();
    nested_declarations(node, &mut nested);
    let prefix = class.name.clone();
    classes.push(class);

    for child in nested {
        collect_types(child, source, file, package, Some(&prefix), classes);
    }
}

fn nested_declarations<'tree>(node: Node<'tree>, found: &mut Vec<Node<'tree>>) {
    let Some(body) = class_body_of(node) else {
        return;
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&child.kind()) {
            found.push(child);
        }
    }
}

fn class_body_of(node: Node) -> Option<Node> {
    find_child(node, "class_body").or_else(|| find_child(node, "enum_class_body"))
}

fn parse_type(
    node: Node,
    source: &[u8],
    file: &str,
    package: Option<&str>,
    outer: Option<&str>,
) -> Option<ClassModel> {
    let is_object = node.kind() == "object_declaration";
    if !is_object && node.kind() != "class_declaration" {
        return None;
    }

    let simple_name = text(node.child_by_field_name("name")?, source);
    let name = match outer {
        Some(prefix) => format!("{prefix}.{simple_name}"),
        None => simple_name.clone(),
    };
    let modifiers = modifiers_of(node, source);
    let (kind, is_abstract) = if is_object {
        (ClassKind::Class, false)
    } else if has_token(node, "interface") {
        (ClassKind::Interface, false)
    } else if modifiers.iter().any(|modifier| modifier == "enum") {
        (ClassKind::Enum, false)
    } else if modifiers.iter().any(|modifier| modifier == "data") {
        (ClassKind::Record, false)
    } else if modifiers.iter().any(|modifier| modifier == "annotation") {
        (ClassKind::Annotation, false)
    } else {
        let is_abstract = modifiers.iter().any(|modifier| modifier == "abstract")
            || modifiers.iter().any(|modifier| modifier == "sealed");
        (ClassKind::Class, is_abstract)
    };

    let mut extends = Vec::new();
    let mut implements = Vec::new();
    if let Some(specifiers) = find_child(node, "delegation_specifiers") {
        let mut cursor = specifiers.walk();
        for specifier in specifiers.named_children(&mut cursor) {
            if specifier.kind() != "delegation_specifier" {
                continue;
            }
            if let Some(invocation) = find_named_child(specifier, "constructor_invocation") {
                if let Some(ty) = find_named_child(invocation, "user_type") {
                    extends.push(text(ty, source));
                }
            } else if let Some(ty) = find_named_child(specifier, "user_type") {
                implements.push(text(ty, source));
            }
        }
    }

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut enum_constants = Vec::new();

    if let Some(parameters) = find_child(node, "primary_constructor")
        .and_then(|constructor| find_named_child(constructor, "class_parameters"))
    {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            if parameter.kind() != "class_parameter" {
                continue;
            }
            let is_property = has_token(parameter, "val") || has_token(parameter, "var");
            if !is_property {
                continue;
            }
            if let Some(field) = field_from_named(parameter, source, is_object) {
                fields.push(field);
            }
        }
    }

    if let Some(body) = class_body_of(node) {
        collect_members(
            body,
            source,
            &simple_name,
            is_object,
            &mut fields,
            &mut methods,
            &mut enum_constants,
        );
    }

    Some(ClassModel {
        name,
        simple_name,
        outer: outer.map(str::to_owned),
        kind,
        package: package.map(str::to_owned),
        is_abstract,
        type_parameters: type_parameters_of(node, source),
        extends,
        implements,
        enum_constants,
        fields,
        methods,
        functions: Vec::new(),
        uses: uses_of(node, source),
        line: node.start_position().row as u32 + 1,
        file: file.to_string(),
    })
}

fn collect_members(
    body: Node,
    source: &[u8],
    class_name: &str,
    static_members: bool,
    fields: &mut Vec<FieldModel>,
    methods: &mut Vec<MethodModel>,
    enum_constants: &mut Vec<EnumConstantModel>,
) {
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "property_declaration" => {
                if let Some(field) = field_from_named(member, source, static_members) {
                    fields.push(field);
                }
            }
            "function_declaration" => {
                methods.push(parse_method(member, source, static_members));
            }
            "secondary_constructor" => {
                methods.push(parse_secondary_constructor(member, source, class_name));
            }
            "enum_entry" => {
                if let Some(name) = find_named_child(member, "identifier") {
                    enum_constants.push(EnumConstantModel {
                        name: text(name, source),
                        line: member.start_position().row as u32 + 1,
                    });
                }
            }
            "companion_object" => {
                if let Some(companion_body) = class_body_of(member) {
                    collect_members(
                        companion_body,
                        source,
                        class_name,
                        true,
                        fields,
                        methods,
                        enum_constants,
                    );
                }
            }
            _ => {}
        }
    }
}

fn field_from_named(node: Node, source: &[u8], is_static: bool) -> Option<FieldModel> {
    let modifiers = modifiers_of(node, source);
    let is_const = modifiers.iter().any(|modifier| modifier == "const");

    let declaration = if node.kind() == "property_declaration" {
        find_named_child(node, "variable_declaration")?
    } else {
        node
    };
    let name = find_named_child(declaration, "identifier").map(|name| text(name, source))?;
    if name.is_empty() {
        return None;
    }
    let type_node = find_child_of_kinds(declaration, TYPE_NODE_KINDS);
    let ty = type_node.map(|node| text(node, source)).unwrap_or_default();
    let (targets, multiplicity) = match type_node {
        Some(node) => type_summary(node, source),
        None => (Vec::new(), Multiplicity::One),
    };

    Some(FieldModel {
        name,
        ty,
        targets,
        multiplicity,
        visibility: visibility(&modifiers),
        is_static: is_static || is_const,
        is_final: is_const || !has_token(node, "var"),
        line: node.start_position().row as u32 + 1,
    })
}

fn parse_method(node: Node, source: &[u8], is_static: bool) -> MethodModel {
    let modifiers = modifiers_of(node, source);
    let name = node
        .child_by_field_name("name")
        .map(|name| text(name, source))
        .unwrap_or_else(|| "?".into());
    let has_body = find_child(node, "function_body").is_some();
    MethodModel {
        name,
        return_ty: return_type_of(node, source),
        visibility: visibility(&modifiers),
        is_static,
        is_abstract: !has_body || modifiers.iter().any(|modifier| modifier == "abstract"),
        is_constructor: false,
        is_async: modifiers.iter().any(|modifier| modifier == "suspend"),
        type_parameters: type_parameters_of(node, source),
        throws: Vec::new(),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn parse_secondary_constructor(node: Node, source: &[u8], class_name: &str) -> MethodModel {
    let modifiers = modifiers_of(node, source);
    MethodModel {
        name: class_name.to_string(),
        return_ty: String::new(),
        visibility: visibility(&modifiers),
        is_static: false,
        is_abstract: false,
        is_constructor: true,
        is_async: false,
        type_parameters: Vec::new(),
        throws: Vec::new(),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn parse_function(node: Node, source: &[u8]) -> FunctionModel {
    let modifiers = modifiers_of(node, source);
    FunctionModel {
        name: node
            .child_by_field_name("name")
            .map(|name| text(name, source))
            .unwrap_or_else(|| "?".into()),
        return_ty: return_type_of(node, source),
        is_async: modifiers.iter().any(|modifier| modifier == "suspend"),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn return_type_of(node: Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    let mut after_parameters = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "function_value_parameters" {
            after_parameters = true;
            continue;
        }
        if after_parameters && child.is_named() && TYPE_NODE_KINDS.contains(&child.kind()) {
            return text(child, source);
        }
        if child.kind() == "function_body" {
            break;
        }
    }
    String::new()
}

fn parse_parameters(node: Node, source: &[u8]) -> Vec<ParameterModel> {
    let Some(parameters) = find_child(node, "function_value_parameters") else {
        return Vec::new();
    };
    let mut collected = Vec::new();
    let mut vararg = false;
    let mut cursor = parameters.walk();
    for child in parameters.named_children(&mut cursor) {
        match child.kind() {
            "parameter_modifiers" => {
                vararg = text(child, source).contains("vararg");
            }
            "parameter" => {
                let name = find_named_child(child, "identifier")
                    .map(|name| text(name, source))
                    .unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let ty = find_child_of_kinds(child, TYPE_NODE_KINDS)
                    .map(|ty| text(ty, source))
                    .unwrap_or_default();
                collected.push(ParameterModel {
                    name: if vararg {
                        format!("vararg {name}")
                    } else {
                        name
                    },
                    ty,
                });
                vararg = false;
            }
            _ => {}
        }
    }
    collected
}

fn collect_module(root: Node, source: &[u8], file: &str, package: Option<&str>) -> ClassModel {
    let stem = module_prefix(file);
    let name = match package {
        Some(package) if !package.is_empty() => format!("{package}.{stem}"),
        _ => stem,
    };
    let mut module = module_model(&name, file);
    module.package = package.map(str::to_owned);
    let mut uses = Vec::new();
    let mut cursor = root.walk();
    for member in root.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&member.kind()) {
            continue;
        }
        match member.kind() {
            "function_declaration" => module.functions.push(parse_function(member, source)),
            "property_declaration" => {
                if let Some(field) = field_from_named(member, source, false) {
                    module.fields.push(field);
                }
            }
            _ => {}
        }
        collect_type_names(member, source, &mut uses);
    }
    module.uses = sorted_unique(uses);
    module
}

fn module_prefix(file: &str) -> String {
    let base = file.rsplit('/').next().unwrap_or(file);
    base.strip_suffix(".kt").unwrap_or(base).to_string()
}

fn type_parameters_of(node: Node, source: &[u8]) -> Vec<String> {
    let Some(parameters) = find_child(node, "type_parameters") else {
        return Vec::new();
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "type_parameter")
        .filter_map(|parameter| find_named_child(parameter, "identifier"))
        .map(|name| text(name, source))
        .collect()
}

fn uses_of(node: Node, source: &[u8]) -> Vec<String> {
    let mut collected = Vec::new();
    collect_type_names(node, source, &mut collected);

    let mut declared = Vec::new();
    collect_type_parameter_names(node, source, &mut declared);
    collected.retain(|name| !declared.contains(name));

    sorted_unique(collected)
}

fn collect_type_names(node: Node, source: &[u8], found: &mut Vec<String>) {
    if node.kind() == "user_type" {
        if let Some(name) = last_identifier(node, source) {
            found.push(name);
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "type_arguments" {
                collect_type_names(child, source, found);
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&child.kind()) {
            continue;
        }
        collect_type_names(child, source, found);
    }
}

fn collect_type_parameter_names(node: Node, source: &[u8], found: &mut Vec<String>) {
    if node.kind() == "type_parameter" {
        if let Some(name) = find_named_child(node, "identifier") {
            found.push(text(name, source));
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&child.kind()) {
            continue;
        }
        collect_type_parameter_names(child, source, found);
    }
}

fn last_identifier(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "identifier")
        .last()
        .map(|name| text(name, source));
    found
}

fn type_summary(node: Node, source: &[u8]) -> (Vec<String>, Multiplicity) {
    let (outer, nullable) = outer_type_info(node, source);
    let is_collection = COLLECTION_TYPES.contains(&outer.as_str());

    let mut targets = Vec::new();
    if is_collection {
        collect_type_names(node, source, &mut targets);
        targets.retain(|name| {
            !COLLECTION_TYPES.contains(&name.as_str()) && !BUILTIN_TYPES.contains(&name.as_str())
        });
        targets.sort();
        targets.dedup();
    } else if !outer.is_empty() && !BUILTIN_TYPES.contains(&outer.as_str()) {
        targets.push(outer);
    }

    let multiplicity = if is_collection {
        Multiplicity::ZeroOrMany
    } else if nullable {
        Multiplicity::ZeroOrOne
    } else {
        Multiplicity::One
    };
    (targets, multiplicity)
}

fn outer_type_info(node: Node, source: &[u8]) -> (String, bool) {
    match node.kind() {
        "nullable_type" => {
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            let name = inner
                .and_then(|child| last_user_type_name(child, source))
                .unwrap_or_default();
            (name, true)
        }
        "user_type" => (last_identifier(node, source).unwrap_or_default(), false),
        _ => (String::new(), false),
    }
}

fn last_user_type_name(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() == "user_type" {
        return last_identifier(node, source);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find_map(|child| last_user_type_name(child, source));
    found
}

fn modifiers_of(node: Node, source: &[u8]) -> Vec<String> {
    find_child(node, "modifiers")
        .map(|modifiers| {
            let mut cursor = modifiers.walk();
            modifiers
                .named_children(&mut cursor)
                .flat_map(|modifier| {
                    if modifier.kind() == "annotation" {
                        Vec::new()
                    } else {
                        text(modifier, source)
                            .split_whitespace()
                            .map(str::to_owned)
                            .collect()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn visibility(modifiers: &[String]) -> Visibility {
    if modifiers.iter().any(|modifier| modifier == "private") {
        Visibility::Private
    } else if modifiers.iter().any(|modifier| modifier == "protected") {
        Visibility::Protected
    } else if modifiers.iter().any(|modifier| modifier == "internal") {
        Visibility::Package
    } else {
        Visibility::Public
    }
}

fn has_token(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == kind);
    found
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    let mut collected: Vec<String> = values
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect();
    collected.sort();
    collected.dedup();
    collected
}

fn find_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn find_named_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn find_child_of_kinds<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

fn text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

fn parse_kotlinc(output: &str, root: &Path) -> Vec<Diagnostic> {
    static CLASSIC: OnceLock<Regex> = OnceLock::new();
    static MODERN: OnceLock<Regex> = OnceLock::new();
    static LEGACY: OnceLock<Regex> = OnceLock::new();

    let classic = CLASSIC.get_or_init(|| {
        Regex::new(r"^(?P<file>.+?):(?P<line>\d+)(?::(?P<col>\d+))?:\s*(?P<sev>error|warning):\s*(?P<msg>.*)$")
            .expect("patrón clásico de kotlinc")
    });
    let modern = MODERN.get_or_init(|| {
        Regex::new(r"^(?P<sev>[ew]):\s*(?:file://)?(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+):?\s*(?P<msg>.*)$")
            .expect("patrón moderno de kotlinc")
    });
    let legacy = LEGACY.get_or_init(|| {
        Regex::new(r"^(?P<sev>[ew]):\s*(?:file://)?(?P<file>.+?):\s*\((?P<line>\d+),\s*(?P<col>\d+)\):\s*(?P<msg>.*)$")
            .expect("patrón intermedio de kotlinc")
    });

    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            for pattern in [classic, legacy, modern] {
                let Some(captures) = pattern.captures(trimmed) else {
                    continue;
                };
                let severity = match captures.name("sev").map(|value| value.as_str()) {
                    Some("w") | Some("warning") => Severity::Warning,
                    _ => Severity::Error,
                };
                let raw_file = captures.name("file")?.as_str();
                return Some(Diagnostic {
                    severity,
                    message: captures.name("msg")?.as_str().trim().to_string(),
                    file: Some(shorten(raw_file, root).unwrap_or_else(|| raw_file.to_string())),
                    line: captures.name("line")?.as_str().parse::<u32>().ok(),
                    column: captures
                        .name("col")
                        .and_then(|value| value.as_str().parse::<u32>().ok()),
                });
            }
            None
        })
        .collect()
}

fn toolchain_crash(output: &str) -> bool {
    output.contains("KotlinFrontEndException")
        || output.contains("Exception while analyzing")
        || output.contains("PermittedSubclasses requires ASM")
        || output.contains("UnsupportedOperationException")
}

fn shorten(path: &str, root: &Path) -> Option<String> {
    let canonical = Path::new(path).canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    canonical
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::project::SourceFile;
    use crate::runner::{self, Stream};
    use std::time::Duration;

    const SAMPLE: &str = r#"
package com.aula

abstract class Animal(val nombre: String, edad: Int) {
    abstract val sonido: String
    var dueno: Dueno? = null
    val juguetes: List<Juguete> = emptyList()

    fun respirar() {}
    protected open fun mover(): Unit {}
}

interface Hablador {
    fun hablar(): String
}

data class Perro(val raza: String) : Animal("x", 4), Hablador {
    override val sonido: String = "Guau"
    override fun hablar(): String = sonido

    companion object {
        const val PATAS = 4
        fun crear(): Perro = Perro("mestizo")
    }

    class Interno
}

enum class Color {
    ROJO,
    VERDE
}

object Registro {
    val todos: MutableList<Perro> = mutableListOf()
    fun agregar(p: Perro) {}
}

fun main(args: Array<String>) {
    println("hola")
}

fun libre(x: Int): Int = x
val CONSTANTE = 5
"#;

    fn parse_one(file: &str, source: &str) -> Vec<ClassModel> {
        parse_files(&[(file, source)]).unwrap()
    }

    fn project_with(relative: &str, content: &str) -> Project {
        Project {
            root: PathBuf::from("/proyecto"),
            sources: vec![SourceFile {
                path: PathBuf::from("/proyecto").join(relative),
                relative: relative.to_string(),
                content: content.to_string(),
            }],
        }
    }

    #[test]
    fn reads_package_abstract_members_and_constructor_properties() {
        let classes = parse_one("Zoo.kt", SAMPLE);
        let animal = classes.iter().find(|c| c.name == "Animal").unwrap();
        assert!(animal.is_abstract);
        assert_eq!(animal.package.as_deref(), Some("com.aula"));
        assert_eq!(animal.line, 4);

        let nombres: Vec<&str> = animal.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(nombres, ["nombre", "sonido", "dueno", "juguetes"]);
        assert!(
            !nombres.contains(&"edad"),
            "un parámetro de constructor sin val/var no es campo"
        );

        let nombre = animal.fields.iter().find(|f| f.name == "nombre").unwrap();
        assert_eq!(nombre.ty, "String");
        assert!(nombre.is_final);
        assert_eq!(nombre.visibility, Visibility::Public);

        let dueno = animal.fields.iter().find(|f| f.name == "dueno").unwrap();
        assert_eq!(dueno.targets, ["Dueno"]);
        assert_eq!(dueno.multiplicity, Multiplicity::ZeroOrOne);
        assert!(!dueno.is_final, "var no es final");

        let juguetes = animal.fields.iter().find(|f| f.name == "juguetes").unwrap();
        assert_eq!(juguetes.targets, ["Juguete"]);
        assert_eq!(juguetes.multiplicity, Multiplicity::ZeroOrMany);

        let mover = animal.methods.iter().find(|m| m.name == "mover").unwrap();
        assert_eq!(mover.visibility, Visibility::Protected);
        assert_eq!(mover.return_ty, "Unit");
        let sonido = animal.fields.iter().find(|f| f.name == "sonido").unwrap();
        assert_eq!(sonido.line, 5);
    }

    #[test]
    fn data_classes_are_records_with_companion_statics_and_nested_types() {
        let classes = parse_one("Zoo.kt", SAMPLE);
        let perro = classes.iter().find(|c| c.name == "Perro").unwrap();
        assert_eq!(perro.kind, ClassKind::Record);
        assert_eq!(perro.extends, ["Animal"]);
        assert_eq!(perro.implements, ["Hablador"]);
        assert_eq!(
            perro.fields.iter().find(|f| f.name == "raza").unwrap().ty,
            "String"
        );

        let patas = perro.fields.iter().find(|f| f.name == "PATAS").unwrap();
        assert!(patas.is_static);
        assert!(patas.is_final);

        let crear = perro.methods.iter().find(|m| m.name == "crear").unwrap();
        assert!(crear.is_static);

        let interno = classes.iter().find(|c| c.name == "Perro.Interno").unwrap();
        assert_eq!(interno.simple_name, "Interno");
        assert_eq!(interno.outer.as_deref(), Some("Perro"));
    }

    #[test]
    fn reads_interfaces_enums_and_objects() {
        let classes = parse_one("Zoo.kt", SAMPLE);

        let hablador = classes.iter().find(|c| c.name == "Hablador").unwrap();
        assert_eq!(hablador.kind, ClassKind::Interface);
        assert!(hablador.methods[0].is_abstract);
        assert_eq!(hablador.methods[0].return_ty, "String");

        let color = classes.iter().find(|c| c.name == "Color").unwrap();
        assert_eq!(color.kind, ClassKind::Enum);
        let constants: Vec<&str> = color
            .enum_constants
            .iter()
            .map(|constant| constant.name.as_str())
            .collect();
        assert_eq!(constants, ["ROJO", "VERDE"]);
        assert_eq!(color.enum_constants[1].line, 31);

        let registro = classes.iter().find(|c| c.name == "Registro").unwrap();
        let agregar = registro
            .methods
            .iter()
            .find(|m| m.name == "agregar")
            .unwrap();
        assert!(agregar.is_static, "los miembros de un object son estáticos");
        let todos = registro.fields.iter().find(|f| f.name == "todos").unwrap();
        assert!(todos.is_static);
        assert_eq!(todos.targets, ["Perro"]);
        assert_eq!(todos.multiplicity, Multiplicity::ZeroOrMany);
    }

    #[test]
    fn module_keeps_top_level_functions_and_constants() {
        let classes = parse_one("Main.kt", SAMPLE);
        let module = classes
            .iter()
            .find(|c| c.kind == ClassKind::Module)
            .unwrap();
        assert_eq!(module.name, "com.aula.Main");
        assert_eq!(module.package.as_deref(), Some("com.aula"));
        let functions: Vec<&str> = module
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect();
        assert_eq!(functions, ["main", "libre"]);
        assert_eq!(module.functions[0].parameters[0].ty, "Array<String>");
        assert_eq!(module.functions[1].return_ty, "Int");
        assert_eq!(
            module
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["CONSTANTE"]
        );
    }

    #[test]
    fn reads_suspend_vararg_and_secondary_constructors() {
        let source = r#"
class S {
    constructor(a: Int)
    suspend fun cargar(): Dueno? = null
    fun varios(vararg nums: Int, f: (Int) -> Int) {}
}
"#;
        let classes = parse_one("s.kt", source);
        let s = &classes[0];

        let constructor = s.methods.iter().find(|m| m.is_constructor).unwrap();
        assert_eq!(constructor.name, "S");
        assert_eq!(constructor.parameters[0].name, "a");
        assert_eq!(constructor.parameters[0].ty, "Int");

        let cargar = s.methods.iter().find(|m| m.name == "cargar").unwrap();
        assert!(cargar.is_async);
        assert_eq!(cargar.return_ty, "Dueno?");

        let varios = s.methods.iter().find(|m| m.name == "varios").unwrap();
        assert_eq!(varios.parameters[0].name, "vararg nums");
        assert_eq!(varios.parameters[0].ty, "Int");
        assert_eq!(varios.parameters[1].ty, "(Int) -> Int");
    }

    #[test]
    fn resolves_supertypes_across_files() {
        let classes = parse_files(&[
            ("a.kt", "class Impl : Base(), If, Otro\n"),
            ("b.kt", "interface If\ninterface Otro\nopen class Base\n"),
        ])
        .unwrap();
        let implementacion = classes.iter().find(|c| c.name == "Impl").unwrap();
        assert_eq!(implementacion.extends, ["Base"]);
        assert_eq!(implementacion.implements, ["If", "Otro"]);

        let if_type = classes.iter().find(|c| c.name == "If").unwrap();
        assert_eq!(if_type.kind, ClassKind::Interface);
    }

    #[test]
    fn nullable_beats_generic_but_collections_win() {
        let source = r#"
class M {
    val a: List<Perro>? = null
    val b: Perro? = null
    val c: Map<String, Gato> = mapOf()
    val d: Repositorio<Mascota>? = null
    val e: Int = 1
    val f = 2
}
"#;
        let classes = parse_one("m.kt", source);
        let m = &classes[0];
        let field = |name: &str| m.fields.iter().find(|f| f.name == name).unwrap();
        assert_eq!(field("a").multiplicity, Multiplicity::ZeroOrMany);
        assert_eq!(field("a").targets, ["Perro"]);
        assert_eq!(field("b").multiplicity, Multiplicity::ZeroOrOne);
        assert_eq!(field("c").targets, ["Gato"]);
        assert_eq!(field("d").targets, ["Repositorio"]);
        assert_eq!(field("d").multiplicity, Multiplicity::ZeroOrOne);
        assert!(field("e").targets.is_empty());
        assert!(field("f").ty.is_empty());
    }

    #[test]
    fn maps_kotlin_visibility_and_finality() {
        let source = r#"
class V {
    private val a: Int = 1
    internal var b: Int = 2
    protected val c: Int = 3
    val d = 4
}
"#;
        let classes = parse_one("v.kt", source);
        let field = |name: &str| classes[0].fields.iter().find(|f| f.name == name).unwrap();
        assert_eq!(field("a").visibility, Visibility::Private);
        assert!(field("a").is_final);
        assert_eq!(field("b").visibility, Visibility::Package);
        assert!(!field("b").is_final);
        assert_eq!(field("c").visibility, Visibility::Protected);
        assert_eq!(field("d").visibility, Visibility::Public);
    }

    #[test]
    fn keeps_type_bounds_and_nested_uses_isolated() {
        let source = r#"
class Caja<T : Comparable<T>, U>(val item: T)

class Outer {
    class Inner {
        val helper: Helper? = null
    }
}
"#;
        let classes = parse_one("c.kt", source);
        let caja = classes.iter().find(|c| c.name == "Caja").unwrap();
        assert_eq!(caja.type_parameters, ["T", "U"]);
        assert_eq!(caja.uses, ["Comparable"]);

        let outer = classes.iter().find(|c| c.name == "Outer").unwrap();
        assert!(outer.uses.is_empty(), "uses del externo: {:?}", outer.uses);
        let inner = classes.iter().find(|c| c.name == "Outer.Inner").unwrap();
        assert_eq!(inner.uses, ["Helper"]);
    }

    #[test]
    fn draws_a_complete_diagram_end_to_end() {
        use crate::core::diagram;

        let source = r#"
package demo

interface Cerrable {
    fun cerrar()
}

abstract class Repositorio<T> {
    val indice: Map<String, List<Mascota>> = mapOf()
    val cache: Mascota? = null
    val elementos: List<Perro> = emptyList()
    private val padre: Repositorio<Mascota>? = null

    class Pagina {
        val filas: List<Perro> = emptyList()
    }
}

open class Mascota

class Perro : Mascota(), Cerrable {
    override fun cerrar() {}
}
"#;
        let classes = parse_files(&[("Demo.kt", source)]).unwrap();
        let mermaid = diagram::to_mermaid(&classes);

        assert!(mermaid.contains("<<interface>>"));
        assert!(mermaid.contains("<<abstract>>"));
        assert!(mermaid.contains("Mascota <|-- Perro"));
        assert!(mermaid.contains("Cerrable <|.. Perro"));
        assert!(mermaid.contains("class Repositorio.Pagina {"));
        assert!(mermaid.contains("Repositorio \"1\" o-- \"0..*\" Mascota : indice"));
        assert!(mermaid.contains("Repositorio \"1\" --> \"0..1\" Mascota : cache"));
        assert!(mermaid.contains("Repositorio \"1\" o-- \"0..*\" Perro : elementos"));
        assert!(mermaid.contains("Repositorio \"1\" --> \"0..1\" Repositorio : padre"));
        assert!(mermaid.contains("Repositorio.Pagina \"1\" o-- \"0..*\" Perro : filas"));
        assert_eq!(mermaid.matches(": padre").count(), 1);
    }

    #[test]
    fn finds_the_facade_class_of_a_top_level_main() {
        let project = project_with("Main.kt", "fun main() {\n    println(\"hola\")\n}\n");
        assert_eq!(detect_entry(&project).unwrap(), "MainKt");

        let project = project_with("app.kt", "fun main(args: Array<String>) {}\n");
        assert_eq!(detect_entry(&project).unwrap(), "AppKt");

        let project = project_with("app.kt", "package com.aula\n\nfun main() {}\n");
        assert_eq!(detect_entry(&project).unwrap(), "com.aula.AppKt");
    }

    #[test]
    fn prefers_the_file_named_main() {
        let project = Project {
            root: PathBuf::from("/proyecto"),
            sources: vec![
                SourceFile {
                    path: PathBuf::from("/proyecto/otra.kt"),
                    relative: "otra.kt".into(),
                    content: "fun main() {}\n".into(),
                },
                SourceFile {
                    path: PathBuf::from("/proyecto/Main.kt"),
                    relative: "Main.kt".into(),
                    content: "fun main() {}\n".into(),
                },
            ],
        };
        assert_eq!(detect_entry(&project).unwrap(), "MainKt");
    }

    #[test]
    fn reports_a_missing_main_function() {
        let project = project_with("Lib.kt", "class Lib\n");
        assert!(matches!(
            detect_entry(&project),
            Err(AdapterError::Invalid(_))
        ));
    }

    #[test]
    fn parses_every_kotlinc_diagnostic_format() {
        let root = Path::new("/proyecto");

        let classic = parse_kotlinc("/proyecto/Foo.kt:5:3: error: unresolved reference: x", root);
        assert_eq!(classic.len(), 1);
        assert_eq!(classic[0].severity, Severity::Error);
        assert_eq!(classic[0].line, Some(5));
        assert_eq!(classic[0].column, Some(3));
        assert_eq!(classic[0].message, "unresolved reference: x");

        let legacy = parse_kotlinc("e: /proyecto/Foo.kt: (5, 3): 'x' overrides nothing", root);
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].line, Some(5));
        assert_eq!(legacy[0].message, "'x' overrides nothing");

        let modern = parse_kotlinc(
            "e: file:///proyecto/Foo.kt:5:3: unresolved reference: x",
            root,
        );
        assert_eq!(modern.len(), 1);
        assert_eq!(modern[0].line, Some(5));

        let warning = parse_kotlinc("w: /proyecto/Foo.kt:2:1: unused variable", root);
        assert_eq!(warning.len(), 1);
        assert_eq!(warning[0].severity, Severity::Warning);
    }

    #[test]
    fn compiles_and_runs_a_real_project_when_kotlinc_is_available() {
        if Command::new(kotlin_tool("kotlinc"))
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-kotlin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Main.kt"),
            "fun main(args: Array<String>) {\n    println(\"hola \" + args.joinToString(\",\"))\n}\n",
        )
        .unwrap();

        let adapter = KotlinAdapter::new();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(build.success, "kotlinc falló: {:?}", build.diagnostics);

        let spec = adapter
            .run_spec(&project, None, &["uno".to_string(), "dos".to_string()])
            .unwrap();
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let outcome = runner::run_streaming(
            &spec,
            Some(Duration::from_secs(30)),
            |stream, line| match stream {
                Stream::Stdout => output.push(line),
                Stream::Stderr => errors.push(line),
            },
        )
        .unwrap();
        assert_eq!(
            outcome.exit_code,
            Some(0),
            "spec: {spec:?}\nsalida: {output:?}\nerrores: {errors:?}"
        );
        assert!(
            output.iter().any(|line| line.contains("hola uno,dos")),
            "salida inesperada: {output:?}"
        );

        fs::write(root.join("Rota.kt"), "fun roto(: Int) {}\n").unwrap();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(!build.success);
        assert!(
            build
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error
                    && diagnostic.file.as_deref() == Some("Rota.kt")),
            "diagnósticos: {:?}",
            build.diagnostics
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn describes_its_own_metadata() {
        let adapter = KotlinAdapter::new();
        assert_eq!(adapter.id(), "kotlin");
        assert_eq!(adapter.compile_label(), "Compilar");
        assert_eq!(adapter.entry_label(), "Archivo principal");
        assert!(adapter.accepts("Clase.kt"));
        assert!(adapter.accepts("CLASE.KT"));
        assert!(!adapter.accepts("Clase.java"));
        assert_eq!(adapter.new_source("Perro.kt"), "class Perro {\n}\n");
        assert!(adapter.validate_new_file("Perro.kt").is_ok());
        assert!(adapter.validate_new_file("_guion.kt").is_ok());
        assert!(adapter.validate_new_file("2mal.kt").is_err());
        assert!(adapter.validate_new_file("mi clase.kt").is_err());
    }

    #[test]
    fn detects_an_incompatible_toolchain_crash() {
        assert!(toolchain_crash(
            "exception: org.jetbrains.kotlin.util.KotlinFrontEndException: Exception while analyzing"
        ));
        assert!(toolchain_crash(
            "Caused by: java.lang.UnsupportedOperationException: PermittedSubclasses requires ASM9"
        ));
        assert!(!toolchain_crash(
            "Foo.kt:1:1: error: unresolved reference: x"
        ));
    }

    #[test]
    fn the_bundled_example_produces_a_complete_diagram() {
        use crate::core::diagram;

        let classes = parse_files(&[
            (
                "Animal.kt",
                include_str!("../../../examples/zoologico_kotlin/Animal.kt"),
            ),
            (
                "Perro.kt",
                include_str!("../../../examples/zoologico_kotlin/Perro.kt"),
            ),
            (
                "Gato.kt",
                include_str!("../../../examples/zoologico_kotlin/Gato.kt"),
            ),
            (
                "Cuidador.kt",
                include_str!("../../../examples/zoologico_kotlin/Cuidador.kt"),
            ),
            (
                "Utilidades.kt",
                include_str!("../../../examples/zoologico_kotlin/Utilidades.kt"),
            ),
            (
                "Main.kt",
                include_str!("../../../examples/zoologico_kotlin/Main.kt"),
            ),
        ])
        .unwrap();

        let animal = classes.iter().find(|c| c.name == "Animal").unwrap();
        assert!(animal.is_abstract);

        let perro = classes.iter().find(|c| c.name == "Perro").unwrap();
        assert_eq!(perro.extends, ["Animal"]);

        let cuidador = classes.iter().find(|c| c.name == "Cuidador").unwrap();
        let animales = cuidador
            .fields
            .iter()
            .find(|f| f.name == "animales")
            .unwrap();
        assert_eq!(animales.targets, ["Animal"]);
        assert_eq!(animales.multiplicity, Multiplicity::ZeroOrMany);

        let utilidades = classes.iter().find(|c| c.name == "Utilidades").unwrap();
        assert_eq!(utilidades.kind, ClassKind::Module);
        let functions: Vec<&str> = utilidades
            .functions
            .iter()
            .map(|function| function.name.as_str())
            .collect();
        assert_eq!(functions, ["describir", "saludar"]);

        let mermaid = diagram::to_mermaid(&classes);
        assert!(mermaid.contains("Animal <|-- Perro"));
        assert!(mermaid.contains("Animal <|-- Gato"));
        assert!(mermaid.contains("Perro \"1\" --> \"1\" Cuidador : cuidador"));
        assert!(mermaid.contains("Cuidador \"1\" o-- \"0..*\" Animal : animales"));
        assert!(mermaid.contains("<<module>>"));
    }

    #[test]
    fn run_spec_requires_compiling_first() {
        let project = project_with("Main.kt", "fun main() {}\n");
        assert!(matches!(
            KotlinAdapter::new().run_spec(&project, None, &[]),
            Err(AdapterError::Invalid(_))
        ));
    }
}
