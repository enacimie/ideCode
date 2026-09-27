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
    ClassKind, ClassModel, EnumConstantModel, FieldModel, MethodModel, Multiplicity,
    ParameterModel, Visibility,
};
use crate::core::project::Project;

const EXTENSIONS: &[&str] = &["java"];
const CLASSES_DIR: &str = ".idecode/classes";

const TYPE_DECLARATIONS: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

const COLLECTION_TYPES: &[&str] = &[
    "Collection",
    "Iterable",
    "List",
    "ArrayList",
    "LinkedList",
    "Set",
    "HashSet",
    "LinkedHashSet",
    "TreeSet",
    "SortedSet",
    "NavigableSet",
    "Queue",
    "Deque",
    "ArrayDeque",
    "PriorityQueue",
    "BlockingQueue",
    "Map",
    "HashMap",
    "LinkedHashMap",
    "TreeMap",
    "SortedMap",
    "NavigableMap",
    "Vector",
    "Stack",
];

const OPTIONAL_TYPES: &[&str] = &["Optional", "OptionalInt", "OptionalLong", "OptionalDouble"];

pub struct JavaAdapter;

impl JavaAdapter {
    pub fn new() -> Self {
        JavaAdapter
    }
}

impl Default for JavaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAdapter for JavaAdapter {
    fn id(&self) -> &'static str {
        "java"
    }

    fn display_name(&self) -> &'static str {
        "Java"
    }

    fn extensions(&self) -> &'static [&'static str] {
        EXTENSIONS
    }

    fn entry_label(&self) -> &'static str {
        "Clase principal"
    }

    fn compile(&self, project: &Project) -> Result<BuildResult, AdapterError> {
        if project.sources.is_empty() {
            return Ok(BuildResult {
                success: false,
                diagnostics: vec![Diagnostic {
                    severity: Severity::Error,
                    message: "No hay archivos .java en el proyecto.".into(),
                    file: None,
                    line: None,
                    column: None,
                }],
            });
        }

        let classes_dir = project.root.join(CLASSES_DIR);
        let _ = fs::remove_dir_all(&classes_dir);
        fs::create_dir_all(&classes_dir)?;

        let javac = tool("javac");
        let mut command = Command::new(&javac);
        command
            .current_dir(&project.root)
            .arg("-d")
            .arg(&classes_dir)
            .arg("-encoding")
            .arg("UTF-8");
        for source in &project.sources {
            command.arg(&source.path);
        }

        let output = command
            .output()
            .map_err(|error| missing_tool(error, &javac, "Java"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

        let mut diagnostics = parse_javac(&stderr, &project.root);
        if diagnostics.is_empty() {
            diagnostics = parse_javac(&stdout, &project.root);
        }
        if !output.status.success() && diagnostics.is_empty() {
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

        let java = tool("java");
        let mut arguments = vec![
            "-Dfile.encoding=UTF-8".to_string(),
            "-Dstdout.encoding=UTF-8".to_string(),
            "-Dstderr.encoding=UTF-8".to_string(),
            "-cp".to_string(),
            classes_dir.to_string_lossy().into_owned(),
            entry,
        ];
        arguments.extend(args.iter().cloned());

        Ok(RunSpec {
            program: java,
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
        format!("public class {class_name} {{\n}}\n")
    }

    fn validate_new_file(&self, name: &str) -> Result<(), String> {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if is_java_identifier(stem) {
            Ok(())
        } else {
            Err(format!(
                "«{stem}» no es un nombre de clase válido en Java. Empieza por letra, «_» o «$» y usa solo letras, dígitos, «_» o «$»."
            ))
        }
    }
}

fn is_java_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let starts_ok = matches!(characters.next(), Some(first) if first == '_' || first == '$' || first.is_alphabetic());
    starts_ok
        && characters
            .all(|character| character == '_' || character == '$' || character.is_alphanumeric())
}

fn tool(name: &str) -> PathBuf {
    let mut extra_dirs = Vec::new();
    if let Ok(home) = std::env::var("JAVA_HOME") {
        if !home.trim().is_empty() {
            let home = Path::new(&home);
            extra_dirs.push(home.join("bin"));
            extra_dirs.push(home.to_path_buf());
        }
    }
    crate::core::tools::find_program(&[name], &extra_dirs).unwrap_or_else(|| PathBuf::from(name))
}

fn missing_tool(error: std::io::Error, tool: &Path, adapter: &str) -> AdapterError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AdapterError::ToolMissing {
            tool: tool.to_string_lossy().into_owned(),
            adapter: adapter.to_string(),
            hint: "Instala un JDK (por ejemplo Eclipse Temurin) y asegúrate de que esté en el PATH o de definir JAVA_HOME.".into(),
        }
    } else {
        AdapterError::Io(error.to_string())
    }
}

fn detect_entry(project: &Project) -> Result<String, AdapterError> {
    let classes = parse_project(project)?;
    let mut candidates: Vec<&ClassModel> =
        classes.iter().filter(|class| class.has_main()).collect();
    if candidates.is_empty() {
        return Err(AdapterError::Invalid(
            "No se encontró ninguna clase con el método main. Añade «public static void main(String[] args)» e inténtalo de nuevo.".into(),
        ));
    }
    if let Some(preferred) = candidates.iter().find(|class| file_stem_matches(class)) {
        return Ok(preferred.qualified_name());
    }
    candidates.sort_by_key(|class| class.qualified_name());
    Ok(candidates[0].qualified_name())
}

fn file_stem_matches(class: &ClassModel) -> bool {
    Path::new(&class.file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == class.file_stem())
}

fn parse_project(project: &Project) -> Result<Vec<ClassModel>, AdapterError> {
    let mut classes = Vec::new();
    for source in &project.sources {
        classes.extend(parse_source(&source.relative, &source.content)?);
    }
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(classes)
}

fn parse_source(file: &str, source: &str) -> Result<Vec<ClassModel>, AdapterError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
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
        collect_types(child, bytes, file, package.as_deref(), None, &mut classes);
    }
    Ok(classes)
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
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if TYPE_DECLARATIONS.contains(&child.kind()) {
            found.push(child);
        } else {
            nested_declarations(child, found);
        }
    }
}

fn parse_type(
    node: Node,
    source: &[u8],
    file: &str,
    package: Option<&str>,
    outer: Option<&str>,
) -> Option<ClassModel> {
    let (kind, body_kind) = match node.kind() {
        "class_declaration" => (ClassKind::Class, "class_body"),
        "interface_declaration" => (ClassKind::Interface, "interface_body"),
        "enum_declaration" => (ClassKind::Enum, "enum_body"),
        "record_declaration" => (ClassKind::Record, "class_body"),
        "annotation_type_declaration" => (ClassKind::Annotation, "annotation_type_body"),
        _ => return None,
    };

    let simple_name = text(node.child_by_field_name("name")?, source);
    let name = match outer {
        Some(prefix) => format!("{prefix}.{simple_name}"),
        None => simple_name.clone(),
    };
    let modifiers = modifiers_of(node, source);
    let is_abstract = modifiers.iter().any(|modifier| modifier == "abstract");
    let type_parameters = type_parameters_of(node, source);

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut enum_constants = Vec::new();

    if kind == ClassKind::Record {
        if let Some(parameters) = node.child_by_field_name("parameters") {
            fields.extend(record_components(parameters, source));
        }
    }

    if let Some(body) = find_child(node, body_kind) {
        if kind == ClassKind::Enum {
            enum_constants = enum_constants_of(body, source);
        }
        let members = if kind == ClassKind::Enum {
            find_child(body, "enum_body_declarations").unwrap_or(body)
        } else {
            body
        };
        collect_members(members, source, kind, &mut fields, &mut methods);
    }

    Some(ClassModel {
        name,
        simple_name,
        outer: outer.map(str::to_owned),
        kind,
        package: package.map(str::to_owned),
        is_abstract,
        type_parameters,
        extends: extends_of(node, source),
        implements: implements_of(node, source),
        enum_constants,
        fields,
        methods,
        functions: Vec::new(),
        uses: uses_of(node, source),
        line: node.start_position().row as u32 + 1,
        file: file.to_string(),
    })
}

fn uses_of(node: Node, source: &[u8]) -> Vec<String> {
    let mut collected = Vec::new();
    collect_type_names(node, source, &mut collected);

    let mut declared = Vec::new();
    collect_type_parameter_names(node, source, &mut declared);
    collected.retain(|name| !declared.contains(name));

    collected.sort();
    collected.dedup();
    collected
}

fn collect_type_names(node: Node, source: &[u8], found: &mut Vec<String>) {
    match node.kind() {
        "type_identifier" => {
            let name = text(node, source);
            if !name.is_empty() {
                found.push(name);
            }
            return;
        }
        "scoped_type_identifier" => {
            let mut cursor = node.walk();
            if let Some(last) = node.named_children(&mut cursor).last() {
                collect_type_names(last, source, found);
            }
            return;
        }
        _ => {}
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
        let mut cursor = node.walk();
        if let Some(name) = node
            .named_children(&mut cursor)
            .find(|child| child.kind() == "type_identifier")
        {
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

fn container_multiplicity(name: &str) -> Option<Multiplicity> {
    if COLLECTION_TYPES.contains(&name) {
        Some(Multiplicity::ZeroOrMany)
    } else if OPTIONAL_TYPES.contains(&name) {
        Some(Multiplicity::ZeroOrOne)
    } else {
        None
    }
}

fn multiplicity_of(node: Node, source: &[u8]) -> Multiplicity {
    if node.kind() == "array_type" {
        return Multiplicity::ZeroOrMany;
    }
    container_multiplicity(&outer_type_name(node, source)).unwrap_or(Multiplicity::One)
}

fn association_targets(node: Node, source: &[u8]) -> Vec<String> {
    let mut targets = Vec::new();
    match node.kind() {
        "array_type" => {
            let mut cursor = node.walk();
            let element = node.named_children(&mut cursor).next();
            if let Some(element) = element {
                collect_type_names(element, source, &mut targets);
            }
        }
        "generic_type" if container_multiplicity(&outer_type_name(node, source)).is_some() => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "type_arguments" {
                    collect_type_names(child, source, &mut targets);
                }
            }
        }
        _ => {
            let name = outer_type_name(node, source);
            if !name.is_empty() {
                targets.push(name);
            }
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn outer_type_name(node: Node, source: &[u8]) -> String {
    let edge = |pick_last: bool| {
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor);
        let chosen = if pick_last {
            children.last()
        } else {
            children.next()
        };
        chosen.map(|child| outer_type_name(child, source))
    };

    match node.kind() {
        "generic_type" | "array_type" => edge(false).unwrap_or_default(),
        "scoped_type_identifier" => edge(true).unwrap_or_default(),
        _ => text(node, source),
    }
}

fn type_parameters_of(node: Node, source: &[u8]) -> Vec<String> {
    let Some(parameters) = node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "type_parameter")
        .filter_map(|parameter| find_named_child(parameter, "type_identifier"))
        .map(|name| text(name, source))
        .collect()
}

fn throws_of(node: Node, source: &[u8]) -> Vec<String> {
    let Some(throws) = find_child(node, "throws") else {
        return Vec::new();
    };
    let mut cursor = throws.walk();
    throws
        .named_children(&mut cursor)
        .map(|exception| text(exception, source))
        .collect()
}

fn enum_constants_of(body: Node, source: &[u8]) -> Vec<EnumConstantModel> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| child.kind() == "enum_constant")
        .filter_map(|constant| {
            let name = constant.child_by_field_name("name")?;
            Some(EnumConstantModel {
                name: text(name, source),
                line: constant.start_position().row as u32 + 1,
            })
        })
        .collect()
}

fn record_components(parameters: Node, source: &[u8]) -> Vec<FieldModel> {
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter(|parameter| parameter.kind() == "formal_parameter")
        .filter_map(|parameter| {
            let name = parameter.child_by_field_name("name")?;
            let ty = parameter.child_by_field_name("type")?;
            Some(FieldModel {
                name: text(name, source),
                ty: text(ty, source),
                targets: association_targets(ty, source),
                multiplicity: multiplicity_of(ty, source),
                visibility: Visibility::Private,
                is_static: false,
                is_final: true,
                line: parameter.start_position().row as u32 + 1,
            })
        })
        .collect()
}

fn collect_members(
    body: Node,
    source: &[u8],
    owner: ClassKind,
    fields: &mut Vec<FieldModel>,
    methods: &mut Vec<MethodModel>,
) {
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "field_declaration" => collect_fields(member, source, owner, fields),
            "method_declaration" => methods.push(parse_method(member, source, owner)),
            "constructor_declaration" => methods.push(parse_constructor(member, source)),
            _ => {}
        }
    }
}

fn collect_fields(node: Node, source: &[u8], owner: ClassKind, fields: &mut Vec<FieldModel>) {
    let modifiers = modifiers_of(node, source);
    let mut visibility = visibility(&modifiers);
    let mut is_static = modifiers.iter().any(|modifier| modifier == "static");
    let mut is_final = modifiers.iter().any(|modifier| modifier == "final");
    if owner == ClassKind::Interface {
        if visibility == Visibility::Package {
            visibility = Visibility::Public;
        }
        is_static = true;
        is_final = true;
    }
    let type_node = node.child_by_field_name("type");
    let ty = type_node
        .map(|node| text(node, source))
        .unwrap_or_else(|| "?".into());
    let targets = type_node
        .map(|node| association_targets(node, source))
        .unwrap_or_default();
    let multiplicity = type_node.map_or(Multiplicity::One, |node| multiplicity_of(node, source));

    let mut cursor = node.walk();
    for declarator in node.children_by_field_name("declarator", &mut cursor) {
        if let Some(name) = declarator.child_by_field_name("name") {
            fields.push(FieldModel {
                name: text(name, source),
                ty: ty.clone(),
                targets: targets.clone(),
                multiplicity,
                visibility,
                is_static,
                is_final,
                line: declarator.start_position().row as u32 + 1,
            });
        }
    }
}

fn parse_method(node: Node, source: &[u8], owner: ClassKind) -> MethodModel {
    let modifiers = modifiers_of(node, source);
    let mut visibility = visibility(&modifiers);
    let mut is_abstract = modifiers.iter().any(|modifier| modifier == "abstract");
    if owner == ClassKind::Interface {
        if visibility == Visibility::Package {
            visibility = Visibility::Public;
        }
        let has_body = node.child_by_field_name("body").is_some();
        let is_default_or_static = modifiers
            .iter()
            .any(|modifier| modifier == "default" || modifier == "static");
        if !has_body && !is_default_or_static {
            is_abstract = true;
        }
    }
    MethodModel {
        name: node
            .child_by_field_name("name")
            .map(|name| text(name, source))
            .unwrap_or_else(|| "?".into()),
        return_ty: node
            .child_by_field_name("type")
            .map(|ty| text(ty, source))
            .unwrap_or_else(|| "void".into()),
        visibility,
        is_static: modifiers.iter().any(|modifier| modifier == "static"),
        is_abstract,
        is_constructor: false,
        is_async: false,
        type_parameters: type_parameters_of(node, source),
        throws: throws_of(node, source),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn parse_constructor(node: Node, source: &[u8]) -> MethodModel {
    let modifiers = modifiers_of(node, source);
    MethodModel {
        name: node
            .child_by_field_name("name")
            .map(|name| text(name, source))
            .unwrap_or_else(|| "?".into()),
        return_ty: String::new(),
        visibility: visibility(&modifiers),
        is_static: false,
        is_abstract: false,
        is_constructor: true,
        is_async: false,
        type_parameters: type_parameters_of(node, source),
        throws: throws_of(node, source),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn parse_parameters(node: Node, source: &[u8]) -> Vec<ParameterModel> {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter_map(|parameter| match parameter.kind() {
            "formal_parameter" => Some(ParameterModel {
                name: parameter
                    .child_by_field_name("name")
                    .map(|name| text(name, source))
                    .unwrap_or_default(),
                ty: parameter
                    .child_by_field_name("type")
                    .map(|ty| text(ty, source))
                    .unwrap_or_default(),
            }),
            "spread_parameter" => Some(ParameterModel {
                name: {
                    let declaration = find_named_child(parameter, "variable_declarator")?;
                    declaration
                        .child_by_field_name("name")
                        .map(|name| text(name, source))
                        .unwrap_or_default()
                },
                ty: format!(
                    "...{}",
                    find_not_kind(parameter, "variable_declarator")
                        .map(|ty| text(ty, source))
                        .unwrap_or_default()
                ),
            }),
            _ => None,
        })
        .collect()
}

fn modifiers_of(node: Node, source: &[u8]) -> Vec<String> {
    find_child(node, "modifiers")
        .map(|modifiers| {
            text(modifiers, source)
                .split_whitespace()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn visibility(modifiers: &[String]) -> Visibility {
    if modifiers.iter().any(|modifier| modifier == "public") {
        Visibility::Public
    } else if modifiers.iter().any(|modifier| modifier == "protected") {
        Visibility::Protected
    } else if modifiers.iter().any(|modifier| modifier == "private") {
        Visibility::Private
    } else {
        Visibility::Package
    }
}

fn extends_of(node: Node, source: &[u8]) -> Vec<String> {
    if let Some(superclass) = node.child_by_field_name("superclass") {
        return first_named_text(superclass, source).into_iter().collect();
    }
    find_child(node, "extends_interfaces")
        .map(|extended| type_list_texts(extended, source))
        .unwrap_or_default()
}

fn implements_of(node: Node, source: &[u8]) -> Vec<String> {
    find_child(node, "super_interfaces")
        .map(|interfaces| type_list_texts(interfaces, source))
        .unwrap_or_default()
}

fn type_list_texts(container: Node, source: &[u8]) -> Vec<String> {
    let Some(list) = find_named_child(container, "type_list") else {
        return Vec::new();
    };
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .map(|node| text(node, source))
        .collect()
}

fn package_name(root: Node, source: &[u8]) -> Option<String> {
    let declaration = find_child(root, "package_declaration")?;
    find_named_child(declaration, "identifier")
        .or_else(|| find_named_child(declaration, "scoped_identifier"))
        .map(|name| text(name, source))
}

fn first_named_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let first = node.named_children(&mut cursor).next();
    first.map(|child| text(child, source))
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

fn find_not_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() != kind);
    found
}

fn text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

fn parse_javac(output: &str, root: &Path) -> Vec<Diagnostic> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"^(?P<file>.+?):(?P<line>\d+):\s+(?P<sev>error|warning|note):\s+(?P<msg>.*)$")
            .expect("a valid javac diagnostic pattern")
    });

    output
        .lines()
        .filter_map(|line| {
            let captures = pattern.captures(line.trim())?;
            let severity = match captures.name("sev")?.as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                _ => Severity::Note,
            };
            let raw_file = captures.name("file")?.as_str();
            Some(Diagnostic {
                severity,
                message: captures.name("msg")?.as_str().trim().to_string(),
                file: Some(shorten(raw_file, root).unwrap_or_else(|| raw_file.to_string())),
                line: captures.name("line")?.as_str().parse::<u32>().ok(),
                column: None,
            })
        })
        .collect()
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
    use std::path::PathBuf;
    use std::time::Duration;

    const SAMPLE: &str = r#"
package com.aula;

import java.util.List;

public class Perro extends Animal implements Serializable {
    private String nombre;
    public static final int PATAS = 4;
    protected List<String> juguetes;

    public Perro(String nombre) {
        this.nombre = nombre;
    }

    private static void adoptar(Animal... mascotas) {
    }
}
"#;

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
    fn reads_package_name_relations_and_members() {
        let classes = parse_source("Perro.java", SAMPLE).unwrap();
        assert_eq!(classes.len(), 1);
        let perro = &classes[0];
        assert_eq!(perro.name, "Perro");
        assert_eq!(perro.line, 6);
        assert_eq!(perro.package.as_deref(), Some("com.aula"));
        assert_eq!(perro.extends, vec!["Animal"]);
        assert_eq!(perro.implements, vec!["Serializable"]);
        assert_eq!(perro.qualified_name(), "com.aula.Perro");

        let nombre = perro.fields.iter().find(|f| f.name == "nombre").unwrap();
        assert_eq!(nombre.visibility, Visibility::Private);
        assert_eq!(nombre.ty, "String");
        assert_eq!(nombre.targets, ["String"]);
        assert_eq!(nombre.multiplicity, Multiplicity::One);

        let patas = perro.fields.iter().find(|f| f.name == "PATAS").unwrap();
        assert!(patas.is_static);
        assert!(patas.is_final);
        assert_eq!(patas.visibility, Visibility::Public);
        assert_eq!(patas.targets, ["int"]);

        let juguetes = perro.fields.iter().find(|f| f.name == "juguetes").unwrap();
        assert_eq!(juguetes.targets, ["String"]);
        assert_eq!(juguetes.multiplicity, Multiplicity::ZeroOrMany);

        let constructor = perro
            .methods
            .iter()
            .find(|m| m.is_constructor)
            .expect("constructor");
        assert_eq!(constructor.name, "Perro");
        assert_eq!(constructor.parameters.len(), 1);
        assert_eq!(constructor.parameters[0].ty, "String");

        let adoptar = perro.methods.iter().find(|m| m.name == "adoptar").unwrap();
        assert!(adoptar.is_static);
        assert_eq!(adoptar.parameters[0].ty, "...Animal");
    }

    #[test]
    fn records_types_used_inside_method_bodies() {
        let source = r#"
class Uso {
    int entero;
    void revisar() {
        Perro perro = new Perro("Toby", 1, "mestizo");
        Mascota mascota = perro;
        java.util.List<Animal> todos = null;
        for (Gato gato : gatos) {
            mascota.jugar();
        }
    }
    void transformar(Map<String, List<Veterinario>> mapa) throws IOException {}
}
"#;
        let classes = parse_source("Uso.java", source).unwrap();
        let uses = &classes[0].uses;
        assert_eq!(
            uses,
            &[
                "Animal",
                "Gato",
                "IOException",
                "List",
                "Map",
                "Mascota",
                "Perro",
                "String",
                "Veterinario"
            ]
        );
        assert!(!uses.iter().any(|ty| ty == "java" || ty == "util"));
        assert!(!uses.iter().any(|ty| ty == "int"));
    }

    #[test]
    fn records_association_targets_and_dependencies() {
        let source = r#"
class Detector {
    Map<String, List<Perro>> registro;
    Optional<Gato> favorito;
    Animal[] manada;
    Repositorio<Gato> repositorio;

    void inspeccionar(Object objeto) {
        Gato gato = (Gato) objeto;
        if (objeto instanceof Mascota) {
            return;
        }
    }
}
"#;
        let classes = parse_source("Detector.java", source).unwrap();
        let detector = &classes[0];

        let registro = detector
            .fields
            .iter()
            .find(|field| field.name == "registro")
            .unwrap();
        assert_eq!(registro.targets, ["List", "Perro", "String"]);
        assert_eq!(registro.multiplicity, Multiplicity::ZeroOrMany);

        let favorito = detector
            .fields
            .iter()
            .find(|field| field.name == "favorito")
            .unwrap();
        assert_eq!(favorito.targets, ["Gato"]);
        assert_eq!(favorito.multiplicity, Multiplicity::ZeroOrOne);

        let manada = detector
            .fields
            .iter()
            .find(|field| field.name == "manada")
            .unwrap();
        assert_eq!(manada.targets, ["Animal"]);
        assert_eq!(manada.multiplicity, Multiplicity::ZeroOrMany);

        let repositorio = detector
            .fields
            .iter()
            .find(|field| field.name == "repositorio")
            .unwrap();
        assert_eq!(repositorio.targets, ["Repositorio"]);
        assert_eq!(repositorio.multiplicity, Multiplicity::One);

        assert!(detector.uses.iter().any(|ty| ty == "Gato"));
        assert!(detector.uses.iter().any(|ty| ty == "Mascota"));
    }

    #[test]
    fn reads_every_superinterface_of_an_interface() {
        let classes = parse_source(
            "Lectura.java",
            "interface Lectura extends Cerrable, java.io.Flushable, AutoCloseable {}",
        )
        .unwrap();
        assert_eq!(
            classes[0].extends,
            ["Cerrable", "java.io.Flushable", "AutoCloseable"]
        );
    }

    #[test]
    fn reads_nested_types_with_qualified_names() {
        let source = r#"
package com.aula;

public class Outer {
    public static class Inner {
        Helper helper;
    }
    interface Contrato {}
}
"#;
        let classes = parse_source("Outer.java", source).unwrap();
        let names: Vec<&str> = classes.iter().map(|class| class.name.as_str()).collect();
        assert_eq!(names, ["Outer", "Outer.Inner", "Outer.Contrato"]);

        let inner = classes
            .iter()
            .find(|class| class.name == "Outer.Inner")
            .unwrap();
        assert_eq!(inner.simple_name, "Inner");
        assert_eq!(inner.outer.as_deref(), Some("Outer"));
        assert_eq!(inner.qualified_name(), "com.aula.Outer$Inner");
        assert_eq!(inner.file_stem(), "Outer");
        assert_eq!(inner.uses, ["Helper"]);

        let outer = classes.iter().find(|class| class.name == "Outer").unwrap();
        assert!(outer.uses.is_empty(), "uses del externo: {:?}", outer.uses);
    }

    #[test]
    fn keeps_type_bounds_but_discards_declared_type_parameters() {
        let source = r#"
class Caja<T extends Comparable<T>, U> {
    <V> V transformar(V valor) { return valor; }
}
"#;
        let classes = parse_source("Caja.java", source).unwrap();
        let caja = &classes[0];
        assert_eq!(caja.type_parameters, ["T", "U"]);
        assert_eq!(caja.uses, ["Comparable"]);
    }

    #[test]
    fn draws_a_complete_diagram_end_to_end() {
        use crate::core::diagram;

        let source = r#"
package demo;

import java.util.List;
import java.util.Map;
import java.util.Optional;

interface Cerrable extends AutoCloseable, Flushable {}

abstract class Repositorio<T> {
    Map<String, List<Mascota>> indice;
    Optional<Mascota> cache;
    Mascota[] elementos;
    private Repositorio<Mascota> padre;

    Mascota buscar(Object clave) {
        if (clave instanceof Perro) {
            Perro perro = (Perro) clave;
            return perro;
        }
        return null;
    }

    static class Pagina {
        List<Perro> filas;
    }
}

class Mascota {}

class Perro extends Mascota implements Cerrable {
    public void close() {}
}
"#;
        let mut classes = parse_source("Demo.java", source).unwrap();
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        let mermaid = diagram::to_mermaid(&classes);

        assert!(mermaid.contains("AutoCloseable <|-- Cerrable"));
        assert!(mermaid.contains("Flushable <|-- Cerrable"));
        assert!(mermaid.contains("Mascota <|-- Perro"));
        assert!(mermaid.contains("Cerrable <|.. Perro"));
        assert!(mermaid.contains("class Repositorio.Pagina {"));
        assert!(mermaid.contains("Repositorio \"1\" o-- \"0..*\" Mascota : indice"));
        assert!(mermaid.contains("Repositorio \"1\" --> \"0..1\" Mascota : cache"));
        assert!(mermaid.contains("Repositorio \"1\" o-- \"0..*\" Mascota : elementos"));
        assert!(mermaid.contains("Repositorio \"1\" --> \"1\" Repositorio : padre"));
        assert!(mermaid.contains("Repositorio.Pagina \"1\" o-- \"0..*\" Perro : filas"));
        assert!(mermaid.contains("Repositorio ..> Perro"));
        assert!(!mermaid.contains("Repositorio ..> Mascota"));
        assert_eq!(mermaid.matches(": padre").count(), 1);
    }

    #[test]
    fn reads_enum_constants_record_components_and_generics() {
        let source = r#"
enum Estado {
    ACTIVO(1),
    INACTIVO(0);

    private final int codigo;

    Estado(int codigo) {
        this.codigo = codigo;
    }

    public int getCodigo() {
        return codigo;
    }
}

record Punto(int x, int y) {}

class Caja<T extends Comparable<T>> {
    <U> U transformar(U valor, T original) throws IOException {
        return valor;
    }
}
"#;
        let classes = parse_source("Tipos.java", source).unwrap();

        let estado = classes.iter().find(|class| class.name == "Estado").unwrap();
        let constants: Vec<&str> = estado
            .enum_constants
            .iter()
            .map(|constant| constant.name.as_str())
            .collect();
        assert_eq!(constants, ["ACTIVO", "INACTIVO"]);
        assert_eq!(estado.enum_constants[0].line, 3);

        let codigo = estado
            .fields
            .iter()
            .find(|field| field.name == "codigo")
            .unwrap();
        assert_eq!(codigo.line, 6);

        let get_codigo = estado
            .methods
            .iter()
            .find(|method| method.name == "getCodigo")
            .unwrap();
        assert_eq!(get_codigo.line, 12);

        let punto = classes.iter().find(|class| class.name == "Punto").unwrap();
        assert_eq!(punto.kind, ClassKind::Record);
        assert_eq!(punto.fields.len(), 2);
        assert!(punto.fields.iter().all(|field| field.is_final));
        assert!(punto
            .fields
            .iter()
            .all(|field| field.visibility == Visibility::Private));

        let caja = classes.iter().find(|class| class.name == "Caja").unwrap();
        assert_eq!(caja.type_parameters, ["T"]);
        let transformar = caja
            .methods
            .iter()
            .find(|method| method.name == "transformar")
            .unwrap();
        assert_eq!(transformar.type_parameters, ["U"]);
        assert_eq!(transformar.throws, ["IOException"]);
    }

    #[test]
    fn marks_abstract_classes() {
        let classes = parse_source(
            "A.java",
            "public abstract class A { public abstract void f(); }",
        )
        .unwrap();
        assert!(classes[0].is_abstract);
    }

    #[test]
    fn reads_interfaces_enums_and_records() {
        let source = r#"
interface Saludable extends Comparable<Saludable> { boolean estaBien(); }
enum Color { ROJO, VERDE }
record Punto(int x, int y) {}
"#;
        let classes = parse_source("Tipos.java", source).unwrap();
        assert_eq!(classes.len(), 3);

        let saludable = classes.iter().find(|c| c.name == "Saludable").unwrap();
        assert_eq!(saludable.kind, ClassKind::Interface);
        assert_eq!(saludable.extends, vec!["Comparable<Saludable>"]);
        assert_eq!(saludable.methods[0].visibility, Visibility::Public);
        assert!(saludable.methods[0].is_abstract);

        let color = classes.iter().find(|c| c.name == "Color").unwrap();
        assert_eq!(color.kind, ClassKind::Enum);
    }

    #[test]
    fn finds_the_main_class_case_insensitively_by_file() {
        let project = project_with(
            "Hola.java",
            "public class Hola { public static void main(String[] args) {} }",
        );
        assert_eq!(detect_entry(&project).unwrap(), "Hola");
    }

    #[test]
    fn prefers_the_class_matching_its_file() {
        let project = project_with(
            "App.java",
            "class Auxiliar { public static void main(String[] args) {} }\npublic class App { public static void main(String[] args) {} }",
        );
        assert_eq!(detect_entry(&project).unwrap(), "App");
    }

    #[test]
    fn reports_a_missing_main_class() {
        let project = project_with("Hola.java", "public class Hola {}");
        assert!(matches!(
            detect_entry(&project),
            Err(AdapterError::Invalid(_))
        ));
    }

    #[test]
    fn parses_javac_diagnostics() {
        let output = "/proyecto/Foo.java:5: error: cannot find symbol\n/proyecto/Foo.java:9: warning: unused";
        let diagnostics = parse_javac(output, Path::new("/proyecto"));
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert_eq!(diagnostics[0].line, Some(5));
        assert_eq!(diagnostics[0].message, "cannot find symbol");
        assert_eq!(diagnostics[1].severity, Severity::Warning);
    }

    #[test]
    fn compiles_and_runs_a_real_project_when_a_jdk_is_available() {
        if Command::new(tool("javac"))
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-java-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Main.java"),
            r#"public class Main {
    public static void main(String[] args) {
        System.out.println("hola " + String.join(",", args));
    }
}
"#,
        )
        .unwrap();

        let adapter = JavaAdapter::new();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(build.success, "javac falló: {:?}", build.diagnostics);

        let spec = adapter
            .run_spec(&project, None, &["uno".to_string(), "dos".to_string()])
            .unwrap();
        let mut output = Vec::new();
        let outcome =
            runner::run_streaming(&spec, Some(Duration::from_secs(20)), |stream, line| {
                if stream == Stream::Stdout {
                    output.push(line);
                }
            })
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(
            output.iter().any(|line| line.contains("hola uno,dos")),
            "salida inesperada: {output:?}"
        );

        fs::write(root.join("Rota.java"), "public class Rota { noExiste(); }").unwrap();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(!build.success);
        assert!(build
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recompiling_removes_stale_classes() {
        if Command::new(tool("javac"))
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-java-stale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Viva.java"), "public class Viva {}").unwrap();
        fs::write(root.join("Muerta.java"), "public class Muerta {}").unwrap();

        let adapter = JavaAdapter::new();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        assert!(adapter.compile(&project).unwrap().success);
        let classes = root.join(CLASSES_DIR);
        assert!(classes.join("Viva.class").is_file());
        assert!(classes.join("Muerta.class").is_file());

        fs::remove_file(root.join("Muerta.java")).unwrap();
        let project = Project::load(&root, EXTENSIONS).unwrap();
        assert!(adapter.compile(&project).unwrap().success);
        assert!(classes.join("Viva.class").is_file());
        assert!(
            !classes.join("Muerta.class").exists(),
            "la clase borrada no debe sobrevivir a la recompilación"
        );

        let _ = fs::remove_dir_all(root);
    }
}
