use std::path::{Path, PathBuf};
use std::process::Command;

use tree_sitter::{Node, Parser};

use crate::core::adapter::{
    AdapterError, BuildResult, Diagnostic, LanguageAdapter, RunSpec, Severity,
};
use crate::core::model::{
    module_model, ClassKind, ClassModel, FieldModel, FunctionModel, MethodModel, Multiplicity,
    ParameterModel, Visibility,
};
use crate::core::project::Project;

const EXTENSIONS: &[&str] = &["py"];

const TYPE_DECLARATIONS: &[&str] = &["class_definition"];

const SYNTAX_CHECK_SCRIPT: &str = r#"
import os
import py_compile
import sys
import tempfile

failed = False
with tempfile.TemporaryDirectory() as scratch:
    for index, path in enumerate(sys.argv[1:]):
        target = os.path.join(scratch, "{0}.pyc".format(index))
        try:
            py_compile.compile(path, cfile=target, doraise=True)
        except Exception as error:
            failed = True
            message = str(error)
            sys.stderr.write(message)
            if not message.endswith("\n"):
                sys.stderr.write("\n")
sys.exit(1 if failed else 0)
"#;

const COLLECTION_TYPES: &[&str] = &[
    "list",
    "List",
    "set",
    "Set",
    "frozenset",
    "FrozenSet",
    "tuple",
    "Tuple",
    "deque",
    "Deque",
    "Sequence",
    "MutableSequence",
    "Iterable",
    "Iterator",
    "Collection",
    "dict_values",
];

const MAPPING_TYPES: &[&str] = &[
    "dict",
    "Dict",
    "Mapping",
    "MutableMapping",
    "defaultdict",
    "Counter",
];

pub struct PythonAdapter;

impl PythonAdapter {
    pub fn new() -> Self {
        PythonAdapter
    }
}

impl Default for PythonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageAdapter for PythonAdapter {
    fn id(&self) -> &'static str {
        "python"
    }

    fn display_name(&self) -> &'static str {
        "Python"
    }

    fn extensions(&self) -> &'static [&'static str] {
        EXTENSIONS
    }

    fn compile_label(&self) -> &'static str {
        "Comprobar sintaxis"
    }

    fn entry_label(&self) -> &'static str {
        "Módulo principal"
    }

    fn compile(&self, project: &Project) -> Result<BuildResult, AdapterError> {
        if project.sources.is_empty() {
            return Ok(BuildResult {
                success: false,
                diagnostics: vec![Diagnostic {
                    severity: Severity::Error,
                    message: "No hay archivos .py en el proyecto.".into(),
                    file: None,
                    line: None,
                    column: None,
                }],
            });
        }

        let python = python_program();
        let mut command = Command::new(&python);
        command
            .arg("-B")
            .arg("-c")
            .arg(SYNTAX_CHECK_SCRIPT)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1");
        for source in &project.sources {
            command.arg(&source.path);
        }

        let output = command
            .output()
            .map_err(|error| missing_tool(error, &python, "Python"))?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let mut diagnostics = parse_python(&stderr, &project.root);
        if !output.status.success() && diagnostics.is_empty() {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: if stderr.trim().is_empty() {
                    "La comprobación de sintaxis falló.".to_string()
                } else {
                    stderr.trim().to_string()
                },
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
        let entry = match entry {
            Some(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => detect_entry(project)?,
        };

        let python = python_program();
        let mut arguments = vec!["-B".to_string(), entry];
        arguments.extend(args.iter().cloned());

        Ok(RunSpec {
            program: python,
            args: arguments,
            cwd: project.root.clone(),
            env: python_env(&project.root),
        })
    }

    fn analyze(&self, project: &Project) -> Result<Vec<ClassModel>, AdapterError> {
        parse_project(project)
    }

    fn new_source(&self, name: &str) -> String {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("nuevo");
        if is_python_identifier(stem) {
            format!("def {stem}():\n    pass\n")
        } else {
            String::from("# Nuevo módulo\n")
        }
    }

    fn validate_new_file(&self, name: &str) -> Result<(), String> {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let valid = is_python_identifier(stem) && stem != "__init__";
        if valid {
            Ok(())
        } else {
            Err(format!(
                "«{stem}» no es un nombre de módulo válido en Python. Usa solo letras, dígitos y «_», y no lo llames __init__."
            ))
        }
    }
}

fn is_python_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let starts_ok =
        matches!(characters.next(), Some(first) if first == '_' || first.is_alphabetic());
    starts_ok && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn python_program() -> PathBuf {
    let mut extra_dirs = Vec::new();
    if let Ok(home) = std::env::var("PYTHON_HOME") {
        if !home.trim().is_empty() {
            let home = Path::new(&home);
            extra_dirs.push(home.join("bin"));
            extra_dirs.push(home.to_path_buf());
        }
    }
    crate::core::tools::find_program(&["python3", "python"], &extra_dirs)
        .unwrap_or_else(|| PathBuf::from("python3"))
}

fn python_env(root: &Path) -> Vec<(String, String)> {
    vec![
        (
            "PYTHONPATH".to_string(),
            root.to_string_lossy().into_owned(),
        ),
        ("PYTHONIOENCODING".to_string(), "utf-8".to_string()),
        ("PYTHONUTF8".to_string(), "1".to_string()),
    ]
}

fn missing_tool(error: std::io::Error, tool: &Path, adapter: &str) -> AdapterError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AdapterError::ToolMissing {
            tool: tool.to_string_lossy().into_owned(),
            adapter: adapter.to_string(),
            hint: "Instala Python 3 (desde python.org o el gestor de paquetes de tu sistema) y asegúrate de que «python3» o «python» esté en el PATH.".into(),
        }
    } else {
        AdapterError::Io(error.to_string())
    }
}

fn detect_entry(project: &Project) -> Result<String, AdapterError> {
    let candidates: Vec<&str> = project
        .sources
        .iter()
        .map(|source| source.relative.as_str())
        .collect();
    if candidates.is_empty() {
        return Err(AdapterError::Invalid(
            "No hay archivos .py en el proyecto.".into(),
        ));
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|relative| relative.eq_ignore_ascii_case("main.py"))
    {
        return Ok((*candidate).to_string());
    }
    if let Some(candidate) = candidates.iter().find(|relative| {
        relative
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("main.py"))
    }) {
        return Ok((*candidate).to_string());
    }

    for source in &project.sources {
        if source.content.contains("__main__") {
            return Ok(source.relative.clone());
        }
    }

    Ok(candidates[0].to_string())
}

fn parse_project(project: &Project) -> Result<Vec<ClassModel>, AdapterError> {
    let mut classes = Vec::new();
    for source in &project.sources {
        classes.extend(parse_source(&source.relative, &source.content)?);
    }
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(classes)
}

fn module_prefix(file: &str) -> String {
    let without_extension = file.strip_suffix(".py").unwrap_or(file);
    let base = without_extension
        .strip_suffix("/__init__")
        .unwrap_or(without_extension);
    base.replace('/', ".")
}

fn parse_source(file: &str, source: &str) -> Result<Vec<ClassModel>, AdapterError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|error| AdapterError::Io(error.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AdapterError::Invalid(format!("No se pudo analizar {file}")))?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let module_name = module_prefix(file);
    let mut classes = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if unwrap_definition(child).kind() == "class_definition" {
            collect_class(child, bytes, file, &module_name, None, &mut classes);
        }
    }

    let module = collect_module(root, bytes, file, &module_name);
    if !module.functions.is_empty() || !module.fields.is_empty() {
        classes.push(module);
    }
    Ok(classes)
}

fn collect_class(
    node: Node,
    source: &[u8],
    file: &str,
    module: &str,
    outer: Option<&str>,
    classes: &mut Vec<ClassModel>,
) {
    let Some(class) = parse_class(node, source, file, module, outer) else {
        return;
    };

    let mut nested = Vec::new();
    nested_declarations(node, &mut nested);
    let prefix = class.name.clone();
    classes.push(class);

    for child in nested {
        collect_class(child, source, file, module, Some(&prefix), classes);
    }
}

fn nested_declarations<'tree>(node: Node<'tree>, found: &mut Vec<Node<'tree>>) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() == "class_definition" {
            found.push(child);
        } else if child.kind() == "decorated_definition" {
            if let Some(definition) = child.child_by_field_name("definition") {
                if definition.kind() == "class_definition" {
                    found.push(definition);
                }
            }
        }
    }
}

fn parse_class(
    node: Node,
    source: &[u8],
    file: &str,
    module: &str,
    outer: Option<&str>,
) -> Option<ClassModel> {
    let node = unwrap_definition(node);
    let simple_name = text(node.child_by_field_name("name")?, source);
    let name = match outer {
        Some(prefix) => format!("{prefix}.{simple_name}"),
        None => simple_name.clone(),
    };
    let type_parameters = type_parameters_of(node, source);
    let (bases, uses) = superclasses_of(node, source);
    let declares_abstract_base = bases.iter().any(|base| base == "ABC" || base == "ABCMeta");
    let extends: Vec<String> = bases
        .into_iter()
        .filter(|base| base != "ABC" && base != "ABCMeta")
        .collect();

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut own_uses = uses;

    if let Some(body) = node.child_by_field_name("body") {
        collect_class_members(body, source, &mut fields, &mut methods, &mut own_uses);
    }

    Some(ClassModel {
        name,
        simple_name,
        outer: outer.map(str::to_owned),
        kind: ClassKind::Class,
        package: if module.is_empty() {
            None
        } else {
            Some(module.to_string())
        },
        is_abstract: declares_abstract_base || methods.iter().any(|method| method.is_abstract),
        type_parameters,
        extends,
        implements: Vec::new(),
        enum_constants: Vec::new(),
        fields: dedupe_fields(fields),
        methods,
        functions: Vec::new(),
        uses: sorted_unique(own_uses),
        line: node.start_position().row as u32 + 1,
        file: file.to_string(),
    })
}

fn dedupe_fields(fields: Vec<FieldModel>) -> Vec<FieldModel> {
    let mut seen = std::collections::HashSet::new();
    fields
        .into_iter()
        .filter(|field| seen.insert(field.name.clone()))
        .collect()
}

fn collect_class_members(
    body: Node,
    source: &[u8],
    fields: &mut Vec<FieldModel>,
    methods: &mut Vec<MethodModel>,
    uses: &mut Vec<String>,
) {
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "function_definition" => {
                methods.push(parse_method(member, source, &[]));
                collect_instance_fields(member, source, fields, uses);
            }
            "decorated_definition" => {
                let Some(definition) = member.child_by_field_name("definition") else {
                    continue;
                };
                if definition.kind() == "function_definition" {
                    let decorators = decorator_names(member, source);
                    methods.push(parse_method(definition, source, &decorators));
                    collect_instance_fields(definition, source, fields, uses);
                }
            }
            "expression_statement" => collect_class_assignments(member, source, &[], fields, uses),
            _ => {}
        }
    }
    collect_type_names(body, source, uses);
}

fn collect_instance_fields(
    function: Node,
    source: &[u8],
    fields: &mut Vec<FieldModel>,
    uses: &mut Vec<String>,
) {
    let Some(body) = function.child_by_field_name("body") else {
        return;
    };
    let parameter_types = parameter_types(function, source);
    let mut cursor = body.walk();
    for statement in body.named_children(&mut cursor) {
        if statement.kind() == "expression_statement" {
            collect_class_assignments(statement, source, &parameter_types, fields, uses);
        }
    }
}

fn parameter_types(function: Node, source: &[u8]) -> Vec<(String, String)> {
    parse_parameters(function, source)
        .into_iter()
        .filter(|parameter| !parameter.ty.is_empty())
        .map(|parameter| (parameter.name, parameter.ty))
        .collect()
}

fn collect_module(root: Node, source: &[u8], file: &str, module_name: &str) -> ClassModel {
    let mut module = module_model(module_name, file);
    let mut uses = Vec::new();
    let mut cursor = root.walk();
    for member in root.named_children(&mut cursor) {
        if unwrap_definition(member).kind() == "class_definition" {
            continue;
        }
        match member.kind() {
            "function_definition" => module.functions.push(parse_function(member, source)),
            "decorated_definition" => {
                let Some(definition) = member.child_by_field_name("definition") else {
                    continue;
                };
                if definition.kind() == "function_definition" {
                    module.functions.push(parse_function(definition, source));
                }
            }
            "expression_statement" => {
                collect_class_assignments(member, source, &[], &mut module.fields, &mut uses);
            }
            _ => {}
        }
        collect_type_names(member, source, &mut uses);
    }
    module.fields = dedupe_fields(module.fields);
    module.uses = sorted_unique(uses);
    module
}

fn collect_class_assignments(
    node: Node,
    source: &[u8],
    parameter_types: &[(String, String)],
    fields: &mut Vec<FieldModel>,
    uses: &mut Vec<String>,
) {
    let mut cursor = node.walk();
    for assignment in node.named_children(&mut cursor) {
        if assignment.kind() != "assignment" {
            continue;
        }
        let Some(left) = assignment.child_by_field_name("left") else {
            continue;
        };
        let name = field_name(left, source);
        if name.is_empty() || name == "__all__" {
            continue;
        }
        let annotated = assignment
            .child_by_field_name("type")
            .map(|annotation| type_text(annotation, source))
            .filter(|annotation| !annotation.is_empty());
        let ty = annotated.or_else(|| inferred_type(assignment, source, parameter_types));
        let (targets, multiplicity) = match &ty {
            Some(annotation) => (
                association_targets(annotation),
                annotation_multiplicity(annotation),
            ),
            None => (Vec::new(), Multiplicity::One),
        };
        if let Some(annotation) = &ty {
            collect_type_names_from_text(annotation, uses);
        }
        fields.push(FieldModel {
            name: name.clone(),
            ty: ty.unwrap_or_default(),
            targets,
            multiplicity,
            visibility: visibility_of(&name),
            is_static: false,
            is_final: false,
            line: assignment.start_position().row as u32 + 1,
        });
    }
}

fn inferred_type(
    assignment: Node,
    source: &[u8],
    parameter_types: &[(String, String)],
) -> Option<String> {
    let right = assignment.child_by_field_name("right")?;
    let identifier = match right.kind() {
        "identifier" => text(right, source),
        "call" => right
            .child_by_field_name("function")
            .map(|function| text(function, source))
            .unwrap_or_default(),
        _ => return None,
    };
    parameter_types
        .iter()
        .find(|(name, _)| *name == identifier)
        .map(|(_, ty)| ty.clone())
}

fn field_name(node: Node, source: &[u8]) -> String {
    match node.kind() {
        "identifier" => text(node, source),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|attribute| text(attribute, source))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn type_text(node: Node, source: &[u8]) -> String {
    let raw = text(node, source);
    if node.kind() == "type" {
        let mut cursor = node.walk();
        let inner = node
            .named_children(&mut cursor)
            .next()
            .map(|child| type_text(child, source))
            .unwrap_or_default();
        return inner;
    }
    let trimmed = raw.strip_prefix(':').map(str::trim).unwrap_or(&raw);
    trimmed.trim().to_string()
}

fn unwrap_definition<'tree>(node: Node<'tree>) -> Node<'tree> {
    if node.kind() == "decorated_definition" {
        if let Some(definition) = node.child_by_field_name("definition") {
            return definition;
        }
    }
    node
}

fn decorator_names(declaration: Node, source: &[u8]) -> Vec<String> {
    let mut cursor = declaration.walk();
    declaration
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "decorator")
        .map(|child| {
            text(child, source)
                .trim_start_matches('@')
                .trim()
                .to_string()
        })
        .collect()
}

fn parse_method(node: Node, source: &[u8], decorators: &[String]) -> MethodModel {
    let name = node
        .child_by_field_name("name")
        .map(|value| text(value, source))
        .unwrap_or_else(|| "?".into());
    let return_ty = node
        .child_by_field_name("return_type")
        .map(|value| text(value, source))
        .unwrap_or_default();
    let is_static = decorators
        .iter()
        .any(|decorator| decorator == "staticmethod");
    let mut parameters = parse_parameters(node, source);
    if !is_static
        && parameters
            .first()
            .is_some_and(|first| first.name == "self" || first.name == "cls")
    {
        parameters.remove(0);
    }

    let is_constructor = name == "__init__";
    MethodModel {
        visibility: visibility_of(&name),
        name,
        return_ty,
        is_static,
        is_abstract: decorators
            .iter()
            .any(|decorator| decorator == "abstractmethod"),
        is_constructor,
        is_async: is_async(node),
        type_parameters: Vec::new(),
        throws: Vec::new(),
        parameters,
        line: node.start_position().row as u32 + 1,
    }
}

fn visibility_of(name: &str) -> Visibility {
    if name.starts_with("__") && name.ends_with("__") {
        Visibility::Public
    } else if name.starts_with("__") {
        Visibility::Private
    } else if name.starts_with('_') {
        Visibility::Protected
    } else {
        Visibility::Public
    }
}

fn parse_function(node: Node, source: &[u8]) -> FunctionModel {
    FunctionModel {
        name: node
            .child_by_field_name("name")
            .map(|value| text(value, source))
            .unwrap_or_else(|| "?".into()),
        return_ty: node
            .child_by_field_name("return_type")
            .map(|value| text(value, source))
            .unwrap_or_default(),
        is_async: is_async(node),
        parameters: parse_parameters(node, source),
        line: node.start_position().row as u32 + 1,
    }
}

fn is_async(node: Node) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| child.kind() == "async");
    found
}

fn parse_parameters(node: Node, source: &[u8]) -> Vec<ParameterModel> {
    let Some(parameters) = node.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter_map(|parameter| match parameter.kind() {
            "identifier" | "typed_parameter" | "typed_default_parameter" => Some(ParameterModel {
                name: parameter_name(parameter, source),
                ty: parameter
                    .child_by_field_name("type")
                    .map(|ty| text(ty, source))
                    .unwrap_or_default(),
            }),
            "default_parameter" => Some(ParameterModel {
                name: parameter
                    .child_by_field_name("name")
                    .map(|name| text(name, source))
                    .unwrap_or_default(),
                ty: String::new(),
            }),
            "list_splat_pattern" => Some(ParameterModel {
                name: format!("*{}", splat_name(parameter, source)),
                ty: String::new(),
            }),
            "dictionary_splat_pattern" => Some(ParameterModel {
                name: format!("**{}", splat_name(parameter, source)),
                ty: String::new(),
            }),
            _ => None,
        })
        .filter(|parameter| {
            !parameter.name.is_empty() && parameter.name != "*" && parameter.name != "**"
        })
        .collect()
}

fn splat_name(node: Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier")
        .map(|child| text(child, source))
        .unwrap_or_default();
    found
}

fn parameter_name(node: Node, source: &[u8]) -> String {
    if node.kind() == "identifier" {
        return text(node, source);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == "identifier")
        .map(|child| text(child, source))
        .unwrap_or_default();
    found
}

fn type_parameters_of(node: Node, source: &[u8]) -> Vec<String> {
    let Some(parameters) = node.child_by_field_name("type_parameters") else {
        return Vec::new();
    };
    let mut cursor = parameters.walk();
    parameters
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "type_parameter")
        .map(|child| text(child, source))
        .collect()
}

fn superclasses_of(node: Node, source: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut bases = Vec::new();
    let Some(arguments) = find_named_child(node, "argument_list") else {
        return (bases, Vec::new());
    };
    let mut uses = Vec::new();
    let mut cursor = arguments.walk();
    for argument in arguments.named_children(&mut cursor) {
        if argument.kind() == "keyword_argument" {
            continue;
        }
        let name = outer_name_of(argument, source);
        if !name.is_empty() && !is_keyword(&name) {
            bases.push(name);
            collect_type_names(argument, source, &mut uses);
        }
    }
    (bases, uses)
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "None" | "True" | "False" | "int" | "float" | "str" | "bool" | "bytes" | "complex"
    )
}

fn collect_type_names(node: Node, source: &[u8], found: &mut Vec<String>) {
    if node.kind() == "identifier" {
        let name = text(node, source);
        if !name.is_empty() {
            found.push(name);
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

fn collect_type_names_from_text(annotation: &str, found: &mut Vec<String>) {
    let trimmed = annotation.trim();
    if trimmed.is_empty() {
        return;
    }
    for name in annotation_names(trimmed) {
        found.push(name);
    }
}

fn annotation_names(annotation: &str) -> Vec<String> {
    let trimmed = annotation.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let union_parts = split_top_level(trimmed, '|');
    if union_parts.len() > 1 {
        return union_parts
            .iter()
            .flat_map(|part| annotation_names(part))
            .collect();
    }

    if let Some(start) = trimmed.find('[') {
        if trimmed.ends_with(']') {
            let mut names = Vec::new();
            let head = trimmed[..start].trim();
            if !head.is_empty() {
                names.push(head.to_string());
            }
            for part in split_top_level(&trimmed[start + 1..trimmed.len() - 1], ',') {
                names.extend(annotation_names(&part));
            }
            return names;
        }
    }

    let comma_parts = split_top_level(trimmed, ',');
    if comma_parts.len() > 1 {
        return comma_parts
            .iter()
            .flat_map(|part| annotation_names(part))
            .collect();
    }

    let cleaned = trimmed.trim_start_matches('*').trim_end_matches('.').trim();
    if cleaned.is_empty() {
        Vec::new()
    } else {
        vec![cleaned.to_string()]
    }
}

fn outer_name_of(node: Node, source: &[u8]) -> String {
    match node.kind() {
        "identifier" => text(node, source),
        "attribute" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .last()
                .map(|child| text(child, source))
                .unwrap_or_default()
        }
        "subscript" => node
            .child_by_field_name("value")
            .map(|value| outer_name_of(value, source))
            .unwrap_or_default(),
        "call" => node
            .child_by_field_name("function")
            .map(|value| outer_name_of(value, source))
            .unwrap_or_default(),
        _ => {
            let mut cursor = node.walk();
            let first = node.named_children(&mut cursor).next();
            first
                .map(|child| outer_name_of(child, source))
                .unwrap_or_default()
        }
    }
}

fn annotation_multiplicity(annotation: &str) -> Multiplicity {
    let base = annotation
        .split('[')
        .next()
        .unwrap_or(annotation)
        .trim()
        .rsplit('.')
        .next()
        .unwrap_or(annotation)
        .trim();
    if COLLECTION_TYPES.contains(&base) || MAPPING_TYPES.contains(&base) {
        Multiplicity::ZeroOrMany
    } else if mentions_none(annotation) {
        Multiplicity::ZeroOrOne
    } else {
        Multiplicity::One
    }
}

fn mentions_none(annotation: &str) -> bool {
    let trimmed = annotation.trim();
    if trimmed.starts_with("Optional[") {
        return true;
    }
    if split_top_level(trimmed, '|')
        .iter()
        .any(|part| part == "None")
    {
        return true;
    }
    if let Some(inner) = trimmed.strip_prefix("Union[") {
        let inner = inner.strip_suffix(']').unwrap_or(inner);
        return split_top_level(inner, ',')
            .iter()
            .any(|part| part == "None");
    }
    false
}

fn split_top_level(input: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for character in input.chars() {
        match character {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            found if found == separator && depth == 0 => {
                parts.push(current.clone());
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    parts.push(current);
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn association_targets(annotation: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for name in annotation_names(annotation) {
        let base = name.rsplit('.').next().unwrap_or(&name).trim().to_string();
        if base.is_empty() || is_keyword(&base) || base == "None" {
            continue;
        }
        if COLLECTION_TYPES.contains(&base.as_str())
            || MAPPING_TYPES.contains(&base.as_str())
            || base == "Optional"
            || base == "Union"
        {
            continue;
        }
        targets.push(base);
    }
    targets.sort();
    targets.dedup();
    targets
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    let mut collected: Vec<String> = values
        .into_iter()
        .filter(|value| !value.is_empty())
        .filter(|value| !is_keyword(value))
        .collect();
    collected.sort();
    collected.dedup();
    collected
}

fn find_named_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

fn parse_python(output: &str, root: &Path) -> Vec<Diagnostic> {
    let lines: Vec<&str> = output.lines().collect();
    let mut diagnostics = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        let Some(location) = trimmed.split_once("File \"") else {
            index += 1;
            continue;
        };
        let (file, rest) = location.1.split_once('"').unwrap_or(("", ""));
        let Some(line_number) = rest
            .trim_start_matches(',')
            .trim_start()
            .strip_prefix("line ")
            .and_then(|value| {
                value
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .ok()
            })
        else {
            index += 1;
            continue;
        };

        let mut message = None;
        let mut lookahead = index + 1;
        while lookahead < lines.len() {
            let candidate = lines[lookahead];
            let is_indented_or_blank =
                candidate.is_empty() || candidate.starts_with(char::is_whitespace);
            if !is_indented_or_blank {
                message = Some(candidate.trim().to_string());
                index = lookahead;
                break;
            }
            if candidate.trim().split_once("File \"").is_some() {
                break;
            }
            lookahead += 1;
        }
        if message.is_none() {
            index = lookahead;
        }

        diagnostics.push(Diagnostic {
            severity: Severity::Error,
            message: message.unwrap_or_else(|| "Error de sintaxis.".to_string()),
            file: Some(shorten(file, root).unwrap_or_else(|| file.to_string())),
            line: Some(line_number),
            column: None,
        });
    }

    diagnostics
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
    use crate::core::model::Multiplicity;
    use crate::core::project::SourceFile;
    use crate::runner::{self, Stream};
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    const SAMPLE: &str = r#"
class Animal:
    def __init__(self, nombre: str, edad: int) -> None:
        self.nombre = nombre
        self.edad = edad

    def hablar(self) -> str:
        return "..."


class Perro(Animal):
    def __init__(self, nombre: str, dueno: Dueno) -> None:
        super().__init__(nombre, 0)
        self.dueno = dueno
        self.juguetes: list[Juguete] = []

    def hablar(self) -> str:
        return "Guau"
"#;

    fn parse_source(file: &str, source: &str) -> Result<Vec<ClassModel>, AdapterError> {
        super::parse_source(file, source)
    }

    #[test]
    fn parses_classes_methods_and_inheritance() {
        let classes = parse_source("animales.py", SAMPLE).unwrap();
        assert_eq!(classes.len(), 2);

        let animal = classes.iter().find(|c| c.name == "Animal").unwrap();
        assert_eq!(
            animal
                .methods
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["__init__", "hablar"]
        );
        let init = &animal.methods[0];
        assert!(init.is_constructor);
        assert_eq!(
            init.parameters
                .iter()
                .map(|p| (p.name.as_str(), p.ty.as_str()))
                .collect::<Vec<_>>(),
            [("nombre", "str"), ("edad", "int")]
        );

        let perro = classes.iter().find(|c| c.name == "Perro").unwrap();
        assert_eq!(perro.extends, ["Animal"]);
        assert_eq!(perro.methods[1].return_ty, "str");
        assert_eq!(perro.methods[0].line, 12);
    }

    #[test]
    fn collects_annotated_attributes_with_multiplicity() {
        let classes = parse_source("perro.py", "class Perro:\n    dueno: Dueno\n    juguetes: list[Juguete] = []\n    viaje: Optional[Dueno] = None\n").unwrap();
        let perro = &classes[0];
        let dueno = perro.fields.iter().find(|f| f.name == "dueno").unwrap();
        assert_eq!(dueno.targets, ["Dueno"]);
        assert_eq!(dueno.multiplicity, Multiplicity::One);
        let juguetes = perro.fields.iter().find(|f| f.name == "juguetes").unwrap();
        assert_eq!(juguetes.targets, ["Juguete"]);
        assert_eq!(juguetes.multiplicity, Multiplicity::ZeroOrMany);
        let viaje = perro.fields.iter().find(|f| f.name == "viaje").unwrap();
        assert_eq!(viaje.multiplicity, Multiplicity::ZeroOrOne);
    }

    #[test]
    fn keeps_top_level_functions_in_a_module() {
        let source = "def saludar(nombre: str) -> None:\n    print(nombre)\n\n\ndef sumar(a: int, b: int) -> int:\n    return a + b\n";
        let classes = parse_source("utilidades.py", source).unwrap();
        assert_eq!(classes.len(), 1);
        let module = &classes[0];
        assert_eq!(module.kind, ClassKind::Module);
        assert_eq!(module.name, "utilidades");
        assert_eq!(
            module
                .functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["saludar", "sumar"]
        );
        assert_eq!(module.functions[1].return_ty, "int");
    }

    #[test]
    fn names_nested_classes_after_their_module_and_outer_class() {
        let source = "class Outer:\n    class Inner:\n        pass\n";
        let classes = parse_source("pkg/mod.py", source).unwrap();
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, "Outer");
        assert_eq!(classes[1].name, "Outer.Inner");
        assert_eq!(classes[1].package.as_deref(), Some("pkg.mod"));
    }

    #[test]
    fn marks_static_and_async_functions() {
        let source = "class C:\n    @staticmethod\n    def crear() -> \"C\":\n        return C()\n\n    async def cargar(self) -> None:\n        pass\n";
        let classes = parse_source("c.py", source).unwrap();
        let class = &classes[0];
        assert!(class.methods[0].is_static);
        assert!(class.methods[0].parameters.is_empty());
        assert!(class.methods[1].is_async);
    }

    #[test]
    fn dedupes_a_field_assigned_in_two_methods() {
        let source = "class D:\n    def __init__(self):\n        self.x: int = 1\n\n    def actualizar(self):\n        self.x = 2\n";
        let classes = parse_source("d.py", source).unwrap();
        let fields: Vec<&str> = classes[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(fields, ["x"]);
        assert_eq!(classes[0].fields[0].ty, "int");
    }

    #[test]
    fn keeps_free_functions_and_constants_alongside_classes() {
        let source = "MAX = 10\n\ndef libre(a: int) -> int:\n    return a\n\nclass C:\n    def metodo(self) -> None:\n        pass\n";
        let classes = parse_source("mixto.py", source).unwrap();
        assert_eq!(classes.len(), 2, "clase + nodo de módulo");

        let clase = classes.iter().find(|c| c.name == "C").unwrap();
        assert_eq!(clase.methods.len(), 1);
        assert!(clase.functions.is_empty());

        let modulo = classes.iter().find(|c| c.name == "mixto").unwrap();
        assert_eq!(modulo.kind, ClassKind::Module);
        assert_eq!(
            modulo
                .functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["libre"]
        );
        assert_eq!(
            modulo
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["MAX"]
        );
    }

    #[test]
    fn detects_decorated_top_level_classes() {
        let source = "from dataclasses import dataclass\n\n@dataclass\nclass Punto:\n    x: int\n    y: int\n";
        let classes = parse_source("punto.py", source).unwrap();
        let punto = classes
            .iter()
            .find(|c| c.name == "Punto")
            .expect("la clase decorada debe aparecer");
        let fields: Vec<(&str, &str)> = punto
            .fields
            .iter()
            .map(|f| (f.name.as_str(), f.ty.as_str()))
            .collect();
        assert_eq!(fields, [("x", "int"), ("y", "int")]);
    }

    #[test]
    fn detects_abstract_via_abc_and_abstractmethod() {
        let source = "from abc import ABC, abstractmethod\n\nclass A(ABC):\n    @abstractmethod\n    def f(self) -> None:\n        ...\n";
        let classes = parse_source("a.py", source).unwrap();
        let a = classes.iter().find(|c| c.name == "A").unwrap();
        assert!(a.is_abstract);
        assert!(a.methods[0].is_abstract);
        assert!(
            !a.extends.iter().any(|base| base == "ABC"),
            "ABC es un marcador, no una superclase dibujable: {:?}",
            a.extends
        );
    }

    #[test]
    fn finds_staticmethod_behind_other_decorators() {
        let source =
            "class S:\n    @classmethod\n    @staticmethod\n    def raro(cls):\n        pass\n";
        let classes = parse_source("s.py", source).unwrap();
        assert!(classes[0].methods[0].is_static);
    }

    #[test]
    fn keeps_star_and_default_parameters() {
        let source = "def f(a, b=1, *args, **kwargs) -> None:\n    pass\n";
        let classes = parse_source("f.py", source).unwrap();
        let names: Vec<&str> = classes[0].functions[0]
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["a", "b", "*args", "**kwargs"]);
    }

    #[test]
    fn distinguishes_optional_from_names_containing_none() {
        let source = "class U:\n    a: NonexistentClass = None\n    b: Dueno | None = None\n    c: Optional[Dueno] = None\n    d: Union[Dueno, None] = None\n";
        let classes = parse_source("u.py", source).unwrap();
        let mult = |name: &str| {
            classes[0]
                .fields
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .multiplicity
        };
        assert_eq!(mult("a"), Multiplicity::One);
        assert_eq!(mult("b"), Multiplicity::ZeroOrOne);
        assert_eq!(mult("c"), Multiplicity::ZeroOrOne);
        assert_eq!(mult("d"), Multiplicity::ZeroOrOne);

        let targets = |name: &str| {
            classes[0]
                .fields
                .iter()
                .find(|f| f.name == name)
                .unwrap()
                .targets
                .clone()
        };
        assert_eq!(targets("b"), ["Dueno"]);
        assert_eq!(targets("d"), ["Dueno"]);
    }

    #[test]
    fn maps_underscore_names_to_visibility() {
        let source = "class V:\n    def __init__(self):\n        self.publico = 1\n        self._interno = 2\n        self.__secreto = 3\n\n    def _protegido(self) -> None:\n        pass\n\n    def __oculto(self) -> None:\n        pass\n";
        let classes = parse_source("v.py", source).unwrap();
        let visibility = |name: &str| {
            classes[0]
                .fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.visibility)
                .or_else(|| {
                    classes[0]
                        .methods
                        .iter()
                        .find(|m| m.name == name)
                        .map(|m| m.visibility)
                })
                .unwrap()
        };
        assert_eq!(visibility("publico"), Visibility::Public);
        assert_eq!(visibility("_interno"), Visibility::Protected);
        assert_eq!(visibility("__secreto"), Visibility::Private);
        assert_eq!(visibility("__init__"), Visibility::Public);
        assert_eq!(visibility("_protegido"), Visibility::Protected);
        assert_eq!(visibility("__oculto"), Visibility::Private);
    }

    #[test]
    fn computes_module_names_from_paths() {
        assert_eq!(module_prefix("main.py"), "main");
        assert_eq!(module_prefix("pkg/utilidades.py"), "pkg.utilidades");
        assert_eq!(module_prefix("pkg/__init__.py"), "pkg");
    }

    #[test]
    fn validates_module_names() {
        let adapter = PythonAdapter::new();
        assert!(adapter.validate_new_file("utilidades.py").is_ok());
        assert!(adapter.validate_new_file("_privado.py").is_ok());
        assert!(adapter.validate_new_file("2modulo.py").is_err());
        assert!(adapter.validate_new_file("__init__.py").is_err());
        assert!(adapter.validate_new_file("mi modulo.py").is_err());
    }

    #[test]
    fn picks_the_main_module_as_entry_point() {
        let project = Project {
            root: PathBuf::from("/tmp"),
            sources: vec![
                SourceFile {
                    path: PathBuf::from("/tmp/otro.py"),
                    relative: "otro.py".into(),
                    content: String::new(),
                },
                SourceFile {
                    path: PathBuf::from("/tmp/main.py"),
                    relative: "main.py".into(),
                    content: String::new(),
                },
            ],
        };
        assert_eq!(detect_entry(&project).unwrap(), "main.py");
    }

    #[test]
    fn runs_a_python_example_when_an_interpreter_is_available() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-python-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.py"),
            "import sys\nprint('hola', sys.argv[1] if len(sys.argv) > 1 else '')\n",
        )
        .unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();

        let build = adapter.compile(&project).unwrap();
        assert!(build.success, "no compila: {:?}", build.diagnostics);

        let spec = adapter
            .run_spec(&project, None, &["mundo".to_string()])
            .unwrap();
        let mut lines = Vec::new();
        let outcome =
            runner::run_streaming(&spec, Some(Duration::from_secs(20)), |stream, line| {
                if stream == Stream::Stdout {
                    lines.push(line);
                }
            })
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(lines.iter().any(|line| line.contains("hola mundo")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_syntax_errors_with_a_line_number() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-python-err-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("roto.py"), "def f(:\n    pass\n").unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(!build.success);
        assert_eq!(build.diagnostics[0].file.as_deref(), Some("roto.py"));
        assert_eq!(build.diagnostics[0].line, Some(1));
        assert!(
            build.diagnostics[0].message.contains("SyntaxError"),
            "el mensaje real debe llegar al alumno: {:?}",
            build.diagnostics[0].message
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reports_every_broken_file_in_one_pass() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let root =
            std::env::temp_dir().join(format!("idecode-python-multi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("roto_a.py"), "def a(:\n    pass\n").unwrap();
        fs::write(root.join("sano.py"), "x = 1\n").unwrap();
        fs::write(root.join("roto_b.py"), "def b(:\n    pass\n").unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        let build = adapter.compile(&project).unwrap();
        assert!(!build.success);

        let files: Vec<&str> = build
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.file.as_deref())
            .collect();
        assert_eq!(files.len(), 2, "diagnósticos: {:?}", build.diagnostics);
        assert!(files.contains(&"roto_a.py"));
        assert!(files.contains(&"roto_b.py"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn syntax_check_does_not_pollute_the_project() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let root =
            std::env::temp_dir().join(format!("idecode-python-limpio-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("sano.py"), "x = 1\n").unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        assert!(adapter.compile(&project).unwrap().success);
        assert!(
            !root.join("__pycache__").exists(),
            "la comprobación no debe crear __pycache__"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_spec_carries_pythonpath_and_utf8() {
        let project = Project {
            root: PathBuf::from("/aula/proyecto"),
            sources: vec![SourceFile {
                path: PathBuf::from("/aula/proyecto/main.py"),
                relative: "main.py".into(),
                content: String::new(),
            }],
        };
        let spec = PythonAdapter::new().run_spec(&project, None, &[]).unwrap();
        assert!(spec
            .env
            .contains(&("PYTHONPATH".to_string(), "/aula/proyecto".to_string())));
        assert!(spec
            .env
            .contains(&("PYTHONIOENCODING".to_string(), "utf-8".to_string())));
    }

    #[test]
    fn a_nested_entry_imports_from_the_project_root() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }

        let root = std::env::temp_dir().join(format!("idecode-python-pkg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("pkg")).unwrap();
        fs::write(root.join("ayuda.py"), "SALUDO = 'hola desde la raiz'\n").unwrap();
        fs::write(
            root.join("pkg").join("app.py"),
            "from ayuda import SALUDO\nprint(SALUDO)\n",
        )
        .unwrap();

        let adapter = PythonAdapter::new();
        let project = Project::load(&root, adapter.extensions()).unwrap();
        let spec = adapter.run_spec(&project, Some("pkg/app.py"), &[]).unwrap();
        let mut lines = Vec::new();
        let outcome =
            runner::run_streaming(&spec, Some(Duration::from_secs(20)), |stream, line| {
                if stream == Stream::Stdout {
                    lines.push(line);
                }
            })
            .unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(lines.iter().any(|line| line.contains("hola desde la raiz")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_python_tracebacks_without_needing_an_interpreter() {
        let root = std::env::temp_dir().join(format!("idecode-parse-py-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let roto = root.join("roto.py");
        let otro = root.join("otro.py");
        fs::write(&roto, "def f(:\n").unwrap();
        fs::write(&otro, "x =\n").unwrap();

        let stderr = format!(
            "  File \"{}\", line 4\n    def f(:\n          ^\nSyntaxError: invalid syntax\n  File \"{}\", line 10\n    x =\n      ^\nSyntaxError: expected expression\n",
            roto.display(),
            otro.display()
        );
        let diagnostics = parse_python(&stderr, &root);
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].file.as_deref(), Some("roto.py"));
        assert_eq!(diagnostics[0].line, Some(4));
        assert_eq!(diagnostics[0].message, "SyntaxError: invalid syntax");
        assert_eq!(diagnostics[1].file.as_deref(), Some("otro.py"));
        assert_eq!(diagnostics[1].line, Some(10));
        assert_eq!(diagnostics[1].message, "SyntaxError: expected expression");

        let _ = fs::remove_dir_all(&root);
    }
}
