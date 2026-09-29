use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::model::{ClassKind, ClassModel, FieldModel, MethodModel, ParameterModel, Visibility};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFile {
    pub relative: String,
    pub content: String,
}

pub fn generate(classes: &[ClassModel], language: &str) -> Result<Vec<GeneratedFile>, String> {
    let context = Context::new(classes)?;
    match language {
        "java" => context.java(),
        "kotlin" => context.kotlin(),
        "python" => context.python(),
        other => Err(format!(
            "No hay generador de código para «{other}». Lenguajes disponibles: Java, Kotlin y Python."
        )),
    }
}

struct Context<'a> {
    classes: &'a [ClassModel],
    by_name: HashMap<&'a str, &'a ClassModel>,
}

impl<'a> Context<'a> {
    fn new(classes: &'a [ClassModel]) -> Result<Self, String> {
        if classes.is_empty() {
            return Err(
                "El diagrama está vacío: añade alguna clase para generar código.".to_string(),
            );
        }

        let mut by_name: HashMap<&'a str, &'a ClassModel> = HashMap::new();
        for class in classes {
            validate_identifier(&class.name)?;
            if by_name.insert(class.name.as_str(), class).is_some() {
                return Err(format!("Hay dos clases llamadas «{}».", class.name));
            }
            match class.kind {
                ClassKind::Class | ClassKind::Interface => {}
                other => {
                    return Err(format!(
                        "«{}»: el diseñador genera clases e interfaces (recibido {other:?}).",
                        class.name
                    ))
                }
            }
            for method in &class.methods {
                validate_identifier(&method.name)?;
                if method.is_constructor {
                    return Err(format!(
                        "«{}.{}»: el diseñador genera los constructores automáticamente a partir de los campos.",
                        class.name, method.name
                    ));
                }
                for parameter in &method.parameters {
                    validate_identifier(&parameter.name)?;
                }
            }
            for field in &class.fields {
                validate_identifier(&field.name)?;
            }
        }

        for class in classes {
            for base in &class.extends {
                if base == &class.name {
                    return Err(format!("«{base}» no puede heredarse de sí misma."));
                }
                if let Some(target) = by_name.get(base.as_str()) {
                    if target.kind == ClassKind::Interface && class.kind == ClassKind::Class {
                        return Err(format!(
                            "«{}» hereda de «{base}», que es una interfaz; usa la relación «implementa».",
                            class.name
                        ));
                    }
                    if target.kind == ClassKind::Class && class.kind == ClassKind::Interface {
                        return Err(format!(
                            "«{}» es una interfaz y solo puede extender interfaces, pero «{base}» es una clase.",
                            class.name
                        ));
                    }
                }
            }
            for interface in &class.implements {
                if let Some(target) = by_name.get(interface.as_str()) {
                    if target.kind == ClassKind::Class {
                        return Err(format!(
                            "«{}» implementa «{interface}», que es una clase; «implementa» solo aplica a interfaces.",
                            class.name
                        ));
                    }
                }
            }
            let mut visited: HashSet<&str> = HashSet::new();
            let mut stack: Vec<&str> = class
                .extends
                .iter()
                .chain(class.implements.iter())
                .map(String::as_str)
                .collect();
            while let Some(name) = stack.pop() {
                if name == class.name {
                    return Err(format!(
                        "La jerarquía de «{}» forma un ciclo de herencia.",
                        class.name
                    ));
                }
                if !visited.insert(name) {
                    continue;
                }
                if let Some(target) = by_name.get(name) {
                    stack.extend(target.extends.iter().map(String::as_str));
                    stack.extend(target.implements.iter().map(String::as_str));
                }
            }
        }

        Ok(Context { classes, by_name })
    }

    fn check_typed_language(&self, language: &str) -> Result<(), String> {
        for class in self.classes {
            if class.kind == ClassKind::Class && class.extends.len() > 1 {
                return Err(format!(
                    "«{}» hereda de varias clases; {language} solo permite una.",
                    class.name
                ));
            }
            for method in &class.methods {
                if method.return_ty.trim().is_empty() {
                    return Err(format!(
                        "«{}.{}» necesita un tipo de retorno.",
                        class.name, method.name
                    ));
                }
                for parameter in &method.parameters {
                    if parameter.ty.trim().is_empty() {
                        return Err(format!(
                            "El parámetro «{}» de «{}.{}» necesita un tipo.",
                            parameter.name, class.name, method.name
                        ));
                    }
                }
            }
            for field in &class.fields {
                if field.ty.trim().is_empty() {
                    return Err(format!(
                        "El campo «{}.{}» necesita un tipo.",
                        class.name, field.name
                    ));
                }
            }
        }
        Ok(())
    }

    fn supertypes_of(&self, class: &ClassModel) -> Vec<&'a ClassModel> {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut found: Vec<&ClassModel> = Vec::new();
        let mut stack: Vec<&str> = class
            .extends
            .iter()
            .chain(class.implements.iter())
            .map(String::as_str)
            .collect();
        while let Some(name) = stack.pop() {
            if !visited.insert(name) {
                continue;
            }
            if let Some(owner) = self.by_name.get(name) {
                found.push(owner);
                stack.extend(owner.extends.iter().map(String::as_str));
                stack.extend(owner.implements.iter().map(String::as_str));
            }
        }
        found
    }

    fn missing_overrides(&self, class: &ClassModel) -> Vec<MethodModel> {
        if class.kind == ClassKind::Interface || class.is_abstract {
            return Vec::new();
        }
        let mut required: Vec<MethodModel> = Vec::new();
        for owner in self.supertypes_of(class) {
            let from_interface = owner.kind == ClassKind::Interface;
            for method in &owner.methods {
                if method.is_static || !(from_interface || method.is_abstract) {
                    continue;
                }
                let signature = |candidate: &MethodModel| {
                    let types = candidate
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty.trim())
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("{}/{types}", candidate.name)
                };
                let wanted = signature(method);
                let matches = |candidates: &[MethodModel]| {
                    candidates
                        .iter()
                        .any(|candidate| signature(candidate) == wanted)
                };
                if matches(&class.methods) || matches(&required) {
                    continue;
                }
                let mut owned = (*method).clone();
                owned.is_abstract = false;
                required.push(owned);
            }
        }
        required
    }

    fn missing_interface_properties(&self, class: &ClassModel) -> Vec<&'a FieldModel> {
        if class.kind == ClassKind::Interface || class.is_abstract {
            return Vec::new();
        }
        let mut required: Vec<&'a FieldModel> = Vec::new();
        for owner in self.supertypes_of(class) {
            if owner.kind != ClassKind::Interface {
                continue;
            }
            for field in &owner.fields {
                if class
                    .fields
                    .iter()
                    .any(|candidate| candidate.name == field.name)
                    || required
                        .iter()
                        .any(|candidate| candidate.name == field.name)
                {
                    continue;
                }
                required.push(field);
            }
        }
        required
    }

    fn java(&self) -> Result<Vec<GeneratedFile>, String> {
        self.check_typed_language("Java")?;
        self.classes
            .iter()
            .map(|class| {
                Ok(GeneratedFile {
                    relative: format!("{}.java", class.name),
                    content: self.java_class(class),
                })
            })
            .collect()
    }

    fn java_class(&self, class: &ClassModel) -> String {
        let keyword = match class.kind {
            ClassKind::Interface => "interface",
            _ => "class",
        };
        let abstract_marker = if class.is_abstract && class.kind == ClassKind::Class {
            "abstract "
        } else {
            ""
        };
        let generics = java_generics(&class.type_parameters);
        let mut header = format!("public {abstract_marker}{keyword} {}{generics}", class.name);

        if class.kind == ClassKind::Interface {
            if !class.extends.is_empty() {
                header.push_str(&format!(" extends {}", class.extends.join(", ")));
            }
        } else {
            if let Some(base) = class.extends.first() {
                header.push_str(&format!(" extends {base}"));
            }
            if !class.implements.is_empty() {
                header.push_str(&format!(" implements {}", class.implements.join(", ")));
            }
        }

        let mut body: Vec<String> = Vec::new();
        for field in &class.fields {
            body.push(java_field(class, field));
        }
        for method in &class.methods {
            body.push(java_method(class, method));
        }
        for method in self.missing_overrides(class) {
            body.push(java_override(&method));
        }

        if body.is_empty() {
            return format!("{header} {{\n}}\n");
        }
        format!("{header} {{\n{}\n}}\n", body.join("\n\n"))
    }

    fn kotlin(&self) -> Result<Vec<GeneratedFile>, String> {
        self.check_typed_language("Kotlin")?;
        self.classes
            .iter()
            .map(|class| {
                Ok(GeneratedFile {
                    relative: format!("{}.kt", class.name),
                    content: self.kotlin_class(class),
                })
            })
            .collect()
    }

    fn kotlin_class(&self, class: &ClassModel) -> String {
        let keyword = match class.kind {
            ClassKind::Interface => "interface",
            _ => "class",
        };
        let abstract_marker = if class.is_abstract && class.kind == ClassKind::Class {
            "abstract "
        } else {
            ""
        };
        let generics = if class.type_parameters.is_empty() {
            String::new()
        } else {
            format!("<{}>", class.type_parameters.join(", "))
        };

        let mut supers: Vec<String> = Vec::new();
        if class.kind == ClassKind::Class {
            if let Some(base) = class.extends.first() {
                supers.push(format!("{base}()"));
            }
            supers.extend(class.implements.iter().cloned());
        } else {
            supers.extend(class.extends.iter().cloned());
        }
        let supertypes = if supers.is_empty() {
            String::new()
        } else {
            format!(" : {}", supers.join(", "))
        };

        let overrides_property = |name: &str| {
            class.kind == ClassKind::Class
                && !class.is_abstract
                && self
                    .missing_interface_properties(class)
                    .iter()
                    .any(|field| field.name == name)
        };

        let mut body: Vec<String> = Vec::new();
        if class.kind == ClassKind::Interface {
            for field in &class.fields {
                body.push(format!(
                    "    val {}: {}",
                    field.name,
                    kotlin_type(&field.ty)
                ));
            }
        } else {
            for field in class.fields.iter().filter(|field| !field.is_static) {
                let ty = kotlin_type(&field.ty);
                let prefix = if overrides_property(&field.name) {
                    "override ".to_string()
                } else {
                    format!("{} ", kotlin_visibility(field.visibility))
                };
                body.push(format!(
                    "    {prefix}{} {}: {} = {}",
                    if field.is_final { "val" } else { "var" },
                    field.name,
                    ty,
                    kotlin_default(&ty)
                ));
            }
        }

        if class.kind == ClassKind::Class {
            let mut companion: Vec<String> = Vec::new();
            for field in class.fields.iter().filter(|field| field.is_static) {
                let ty = kotlin_type(&field.ty);
                companion.push(format!(
                    "        {} {} {}: {} = {}",
                    kotlin_visibility(field.visibility),
                    if field.is_final { "val" } else { "var" },
                    field.name,
                    ty,
                    kotlin_default(&ty)
                ));
            }
            for method in class.methods.iter().filter(|method| method.is_static) {
                companion.push(kotlin_static_function(method));
            }
            if !companion.is_empty() {
                body.push(format!(
                    "    companion object {{\n{}\n    }}",
                    companion.join("\n\n")
                ));
            }
        }

        for method in &class.methods {
            if class.kind == ClassKind::Class && method.is_static {
                continue;
            }
            let declaration_only = class.kind == ClassKind::Interface;
            let abstract_method =
                method.is_abstract && class.is_abstract && class.kind == ClassKind::Class;
            body.push(kotlin_function(method, declaration_only, abstract_method));
        }

        for method in self.missing_overrides(class) {
            body.push(kotlin_override(&method));
        }

        for field in self.missing_interface_properties(class) {
            let ty = kotlin_type(&field.ty);
            body.push(format!(
                "    override val {}: {} = {}",
                field.name,
                ty,
                kotlin_default(&ty)
            ));
        }

        if body.is_empty() {
            return format!(
                "{abstract_marker}{keyword} {}{generics}{supertypes}\n",
                class.name
            );
        }
        format!(
            "{abstract_marker}{keyword} {}{generics}{supertypes} {{\n{}\n}}\n",
            class.name,
            body.join("\n\n")
        )
    }

    fn python_imports(&self, class: &ClassModel) -> Vec<String> {
        let mut needed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut request = |name: &str| {
            if name != class.name && self.by_name.contains_key(name) {
                needed.insert(name.to_string());
            }
        };
        for base in class.extends.iter().chain(class.implements.iter()) {
            request(base);
        }
        let mut types: Vec<&str> = Vec::new();
        for field in &class.fields {
            types.push(field.ty.as_str());
        }
        for method in &class.methods {
            types.push(method.return_ty.as_str());
            for parameter in &method.parameters {
                types.push(parameter.ty.as_str());
            }
        }
        for ty in types {
            for name in self.by_name.keys() {
                if type_mentions(ty, name) {
                    request(name);
                }
            }
        }
        needed
            .into_iter()
            .map(|name| format!("from {} import {name}", snake_case(&name)))
            .collect()
    }

    fn python(&self) -> Result<Vec<GeneratedFile>, String> {
        self.classes
            .iter()
            .map(|class| {
                Ok(GeneratedFile {
                    relative: format!("{}.py", snake_case(&class.name)),
                    content: self.python_class(class),
                })
            })
            .collect()
    }

    fn python_class(&self, class: &ClassModel) -> String {
        let is_interface = class.kind == ClassKind::Interface;
        let needs_abc = is_interface
            || class.is_abstract
            || class.methods.iter().any(|method| method.is_abstract);
        let mut supers: Vec<String> = Vec::new();
        supers.extend(class.extends.iter().cloned());
        supers.extend(class.implements.iter().cloned());
        if needs_abc && !supers.iter().any(|base| base == "ABC") {
            supers.push("ABC".to_string());
        }
        let header = if supers.is_empty() {
            format!("class {}:", class.name)
        } else {
            format!("class {}({}):", class.name, supers.join(", "))
        };

        let mut body: Vec<String> = Vec::new();
        let mut main: Option<MethodModel> = None;

        for field in &class.fields {
            if field.is_static || is_interface {
                body.push(python_class_attribute(field));
            }
        }

        if !is_interface {
            let instance_fields: Vec<&FieldModel> = class
                .fields
                .iter()
                .filter(|field| !field.is_static)
                .collect();
            if !instance_fields.is_empty() {
                body.push(python_init(&instance_fields));
            }
        }

        for method in &class.methods {
            if method.is_static && method.name == "main" {
                main = Some((*method).clone());
                continue;
            }
            body.push(python_method(class, method, needs_abc));
        }
        for method in self.missing_overrides(class) {
            body.push(python_plain_method(&method));
        }

        let mut output = String::new();
        let mut imports: Vec<String> = self.python_imports(class);
        if needs_abc {
            imports.insert(0, "from abc import ABC, abstractmethod".to_string());
        }
        if !imports.is_empty() {
            output.push_str(&imports.join("\n"));
            output.push_str("\n\n\n");
        }
        output.push_str(&header);
        output.push('\n');
        if body.is_empty() {
            output.push_str("    pass\n");
        } else {
            output.push_str(&body.join("\n\n"));
            output.push('\n');
        }

        if let Some(method) = main {
            output.push_str("\n\n");
            output.push_str(&python_module_main(&method));
            output.push_str("\n\n\nif __name__ == \"__main__\":\n    main()\n");
        }

        output
    }
}

fn validate_identifier(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let valid = match chars.next() {
        Some(first) if first == '_' || first.is_alphabetic() => {
            chars.all(|character| character == '_' || character.is_alphanumeric())
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "«{name}» no es un identificador válido: usa letras, dígitos y «_», sin empezar por dígito."
        ))
    }
}

fn java_visibility(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public ",
        Visibility::Protected => "protected ",
        Visibility::Package => "",
        Visibility::Private => "private ",
    }
}

fn kotlin_visibility(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Package => "internal",
        Visibility::Private => "private",
    }
}

fn java_generics(type_parameters: &[String]) -> String {
    if type_parameters.is_empty() {
        String::new()
    } else {
        format!("<{}>", type_parameters.join(", "))
    }
}

fn java_parameters(parameters: &[ParameterModel]) -> String {
    parameters
        .iter()
        .map(|parameter| format!("{} {}", parameter.ty.trim(), parameter.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn java_default(ty: &str) -> String {
    match ty.trim().to_lowercase().as_str() {
        "int" | "short" | "byte" => "0".to_string(),
        "long" => "0L".to_string(),
        "double" => "0.0".to_string(),
        "float" => "0.0f".to_string(),
        "boolean" => "false".to_string(),
        "char" => "' '".to_string(),
        "string" => "\"\"".to_string(),
        _ => "null".to_string(),
    }
}

fn java_field(class: &ClassModel, field: &FieldModel) -> String {
    if class.kind == ClassKind::Interface {
        return format!(
            "    public static final {} {} = {};",
            field.ty.trim(),
            field.name,
            java_default(&field.ty)
        );
    }
    let mut modifiers = String::from(java_visibility(field.visibility));
    if field.is_static {
        modifiers.push_str("static ");
    }
    if field.is_final {
        modifiers.push_str("final ");
    }
    let initializer = if field.is_final {
        format!(" = {}", java_default(&field.ty))
    } else {
        String::new()
    };
    format!(
        "    {modifiers}{} {}{};",
        field.ty.trim(),
        field.name,
        initializer
    )
}

fn java_method(class: &ClassModel, method: &MethodModel) -> String {
    let parameters = java_parameters(&method.parameters);
    let generics = java_generics(&method.type_parameters);
    let throws = if method.throws.is_empty() {
        String::new()
    } else {
        format!(" throws {}", method.throws.join(", "))
    };
    let return_ty = method.return_ty.trim();

    if class.kind == ClassKind::Interface {
        return format!(
            "    {return_ty} {}{generics}({parameters}){throws};",
            method.name
        );
    }

    let mut modifiers = String::from(java_visibility(method.visibility));
    if method.is_static {
        modifiers.push_str("static ");
    }
    let abstract_method = method.is_abstract && class.is_abstract;
    if abstract_method {
        modifiers.push_str("abstract ");
        return format!(
            "    {modifiers}{return_ty} {}{generics}({parameters}){throws};",
            method.name
        );
    }

    format!(
        "    {modifiers}{return_ty} {}{generics}({parameters}){throws} {{\n        throw new UnsupportedOperationException(\"TODO\");\n    }}"
    ,
        method.name
    )
}

fn java_override(method: &MethodModel) -> String {
    format!(
        "    @Override\n    {}{} {}({}) {{\n        throw new UnsupportedOperationException(\"TODO\");\n    }}",
        java_visibility(method.visibility),
        method.return_ty.trim(),
        method.name,
        java_parameters(&method.parameters)
    )
}

fn kotlin_type(ty: &str) -> String {
    let trimmed = ty.trim();
    if let Some(inner) = trimmed.strip_suffix("[]") {
        return format!("List<{}>", kotlin_type(inner));
    }
    if let Some(rest) = trimmed.strip_prefix("List<") {
        if let Some(inner) = rest.strip_suffix('>') {
            return format!("List<{}>", kotlin_type(inner));
        }
    }
    match trimmed.to_lowercase().as_str() {
        "int" => "Int",
        "boolean" => "Boolean",
        "double" => "Double",
        "float" => "Float",
        "long" => "Long",
        "short" => "Short",
        "byte" => "Byte",
        "char" => "Char",
        "string" => "String",
        "void" => "Unit",
        "object" => "Any",
        _ => trimmed,
    }
    .to_string()
}

fn kotlin_default(ty: &str) -> String {
    match ty {
        "Int" | "Short" | "Byte" | "Long" => "0",
        "Double" => "0.0",
        "Float" => "0.0f",
        "Boolean" => "false",
        "Char" => "' '",
        "String" => "\"\"",
        other if other.starts_with("List<") => "emptyList()",
        _ => "TODO()",
    }
    .to_string()
}

fn kotlin_parameters(parameters: &[ParameterModel]) -> String {
    parameters
        .iter()
        .map(|parameter| format!("{}: {}", parameter.name, kotlin_type(&parameter.ty)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn kotlin_return(return_ty: &str) -> String {
    let mapped = kotlin_type(return_ty);
    if mapped == "Unit" {
        String::new()
    } else {
        format!(": {mapped}")
    }
}

fn kotlin_visibility_prefix(method: &MethodModel) -> String {
    match method.visibility {
        Visibility::Public => String::new(),
        other => format!("{} ", kotlin_visibility(other)),
    }
}

fn kotlin_function(method: &MethodModel, declaration_only: bool, abstract_method: bool) -> String {
    let generics = if method.type_parameters.is_empty() {
        String::new()
    } else {
        format!("<{}> ", method.type_parameters.join(", "))
    };
    let visibility = if declaration_only {
        String::new()
    } else {
        kotlin_visibility_prefix(method)
    };
    let abstract_marker = if abstract_method { "abstract " } else { "" };
    let signature = format!(
        "    {visibility}{abstract_marker}{generics}fun {}({}){}",
        method.name,
        kotlin_parameters(&method.parameters),
        kotlin_return(&method.return_ty)
    );
    if declaration_only || abstract_method {
        return signature;
    }
    format!("{signature} {{\n        TODO(\"Implementar\")\n    }}")
}

fn kotlin_static_function(method: &MethodModel) -> String {
    format!(
        "        {}fun {}({}){} {{\n            TODO(\"Implementar\")\n        }}",
        kotlin_visibility_prefix(method),
        method.name,
        kotlin_parameters(&method.parameters),
        kotlin_return(&method.return_ty)
    )
}

fn kotlin_override(method: &MethodModel) -> String {
    format!(
        "    {}override fun {}({}){} {{\n        TODO(\"Implementar\")\n    }}",
        kotlin_visibility_prefix(method),
        method.name,
        kotlin_parameters(&method.parameters),
        kotlin_return(&method.return_ty)
    )
}

fn python_type(ty: &str) -> String {
    let trimmed = ty.trim();
    if trimmed.ends_with("[]") {
        return "list".to_string();
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("list<") {
        return "list".to_string();
    }
    if lower.starts_with("set<") {
        return "set".to_string();
    }
    if lower.starts_with("map<") {
        return "dict".to_string();
    }
    match lower.as_str() {
        "int" | "long" | "short" | "byte" => "int",
        "string" | "char" => "str",
        "boolean" => "bool",
        "double" | "float" => "float",
        "void" => "None",
        "object" => "object",
        _ => trimmed,
    }
    .to_string()
}

fn python_default(ty: &str) -> Option<String> {
    match ty {
        "int" => Some("0".to_string()),
        "float" => Some("0.0".to_string()),
        "str" => Some("\"\"".to_string()),
        "bool" => Some("False".to_string()),
        _ => None,
    }
}

fn python_prefix(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "__",
        Visibility::Protected => "_",
        _ => "",
    }
}

fn python_annotation(ty: &str) -> String {
    let trimmed = ty.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!(": {}", python_type(trimmed))
}

fn python_return(return_ty: &str) -> String {
    let trimmed = return_ty.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    format!(" -> {}", python_type(trimmed))
}

fn python_parameters(parameters: &[ParameterModel]) -> Vec<String> {
    parameters
        .iter()
        .map(|parameter| format!("{}{}", parameter.name, python_annotation(&parameter.ty)))
        .collect()
}

fn python_class_attribute(field: &FieldModel) -> String {
    if field.ty.trim().is_empty() {
        return format!("    {} = None", field.name);
    }
    let mapped = python_type(&field.ty);
    match python_default(&mapped) {
        Some(literal) => format!("    {}: {} = {}", field.name, mapped, literal),
        None => format!("    {} = None", field.name),
    }
}

fn python_init(fields: &[&FieldModel]) -> String {
    let parameters = fields
        .iter()
        .map(|field| format!("{}{}", field.name, python_annotation(&field.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut output = format!("    def __init__(self, {parameters}):\n");
    for field in fields {
        output.push_str(&format!(
            "        self.{}{} = {}\n",
            python_prefix(field.visibility),
            field.name,
            field.name
        ));
    }
    output.trim_end().to_string()
}

fn python_method(class: &ClassModel, method: &MethodModel, needs_abc: bool) -> String {
    let abstract_method = class.kind == ClassKind::Interface || (method.is_abstract && needs_abc);
    let mut decorators = String::new();
    if method.is_static {
        decorators.push_str("    @staticmethod\n");
    }
    if abstract_method {
        decorators.push_str("    @abstractmethod\n");
    }
    let mut parameters = Vec::new();
    if !method.is_static {
        parameters.push("self".to_string());
    }
    parameters.extend(python_parameters(&method.parameters));
    format!(
        "{decorators}    def {}({}){}:\n        raise NotImplementedError(\"TODO\")",
        method.name,
        parameters.join(", "),
        python_return(&method.return_ty)
    )
}

fn python_plain_method(method: &MethodModel) -> String {
    let decorators = if method.is_static {
        "    @staticmethod\n"
    } else {
        ""
    };
    let mut parameters = Vec::new();
    if !method.is_static {
        parameters.push("self".to_string());
    }
    parameters.extend(python_parameters(&method.parameters));
    format!(
        "{decorators}    def {}({}){}:\n        raise NotImplementedError(\"TODO\")",
        method.name,
        parameters.join(", "),
        python_return(&method.return_ty)
    )
}

fn python_module_main(method: &MethodModel) -> String {
    format!(
        "def main({}){}:\n    raise NotImplementedError(\"TODO\")",
        python_parameters(&method.parameters).join(", "),
        python_return(&method.return_ty)
    )
}

fn is_identifier_char(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn type_mentions(ty: &str, name: &str) -> bool {
    let haystack: Vec<char> = ty.chars().collect();
    let needle: Vec<char> = name.chars().collect();
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    (0..=haystack.len() - needle.len()).any(|start| {
        haystack[start..start + needle.len()] == needle[..]
            && (start == 0 || !is_identifier_char(haystack[start - 1]))
            && (start + needle.len() == haystack.len()
                || !is_identifier_char(haystack[start + needle.len()]))
    })
}

pub fn snake_case(name: &str) -> String {
    let characters: Vec<char> = name.chars().collect();
    let mut output = String::new();
    for (index, character) in characters.iter().enumerate() {
        if character.is_uppercase() {
            let previous_lower = index > 0
                && (characters[index - 1].is_lowercase() || characters[index - 1].is_numeric());
            let next_lower = index + 1 < characters.len() && characters[index + 1].is_lowercase();
            if index > 0 && (previous_lower || next_lower) {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(*character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_class(name: &str, kind: ClassKind, is_abstract: bool) -> ClassModel {
        ClassModel {
            name: name.to_string(),
            simple_name: name.to_string(),
            outer: None,
            kind,
            package: None,
            is_abstract,
            type_parameters: Vec::new(),
            extends: Vec::new(),
            implements: Vec::new(),
            enum_constants: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            functions: Vec::new(),
            uses: Vec::new(),
            line: 1,
            file: String::new(),
        }
    }

    fn field(name: &str, ty: &str, visibility: Visibility) -> FieldModel {
        FieldModel {
            name: name.to_string(),
            ty: ty.to_string(),
            targets: Vec::new(),
            multiplicity: Default::default(),
            visibility,
            is_static: false,
            is_final: false,
            line: 1,
        }
    }

    fn method(name: &str, return_ty: &str, params: Vec<(&str, &str)>) -> MethodModel {
        MethodModel {
            name: name.to_string(),
            return_ty: return_ty.to_string(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_async: false,
            type_parameters: Vec::new(),
            throws: Vec::new(),
            parameters: params
                .into_iter()
                .map(|(name, ty)| ParameterModel {
                    name: name.to_string(),
                    ty: ty.to_string(),
                })
                .collect(),
            line: 1,
        }
    }

    fn zoo() -> Vec<ClassModel> {
        let mut mascota = base_class("Mascota", ClassKind::Interface, false);
        mascota.methods.push(method("ladrar", "String", vec![]));

        let mut animal = base_class("Animal", ClassKind::Class, true);
        animal
            .fields
            .push(field("patas", "int", Visibility::Protected));
        let mut mover = method("mover", "void", vec![]);
        mover.is_abstract = true;
        animal.methods.push(mover);

        let mut perro = base_class("Perro", ClassKind::Class, false);
        perro.extends.push("Animal".to_string());
        perro.implements.push("Mascota".to_string());
        perro
            .fields
            .push(field("nombre", "String", Visibility::Private));
        perro
            .methods
            .push(method("saludar", "void", vec![("texto", "String")]));

        vec![mascota, animal, perro]
    }

    fn content_of(files: &[GeneratedFile], relative: &str) -> String {
        files
            .iter()
            .find(|file| file.relative == relative)
            .unwrap_or_else(|| panic!("falta {relative}"))
            .content
            .clone()
    }

    #[test]
    fn java_generates_skeleton_with_automatic_overrides() {
        let files = generate(&zoo(), "java").expect("generación java");
        assert_eq!(
            files
                .iter()
                .map(|file| file.relative.clone())
                .collect::<Vec<_>>(),
            vec!["Mascota.java", "Animal.java", "Perro.java"]
        );

        let mascota = content_of(&files, "Mascota.java");
        assert!(mascota.contains("public interface Mascota {"));
        assert!(mascota.contains("    String ladrar();"));

        let animal = content_of(&files, "Animal.java");
        assert!(animal.contains("public abstract class Animal {"));
        assert!(animal.contains("    protected int patas;"));
        assert!(animal.contains("    public abstract void mover();"));

        let perro = content_of(&files, "Perro.java");
        assert!(perro.contains("public class Perro extends Animal implements Mascota {"));
        assert!(perro.contains("    private String nombre;"));
        assert!(perro.contains("    public void saludar(String texto) {"));
        assert!(perro.contains("throw new UnsupportedOperationException(\"TODO\");"));
        assert!(perro.contains("    @Override\n    public String ladrar() {"));
        assert!(perro.contains("    @Override\n    public void mover() {"));
    }

    #[test]
    fn kotlin_generates_properties_companion_and_overrides() {
        let mut classes = zoo();
        {
            let perro = classes
                .iter_mut()
                .find(|class| class.name == "Perro")
                .unwrap();
            let mut especie = field("ESPECIE", "String", Visibility::Public);
            especie.is_static = true;
            especie.is_final = true;
            perro.fields.push(especie);
            let mut main = method("main", "void", vec![]);
            main.is_static = true;
            perro.methods.push(main);
        }

        let files = generate(&classes, "kotlin").expect("generación kotlin");

        let mascota = content_of(&files, "Mascota.kt");
        assert!(mascota.contains("interface Mascota {"));
        assert!(mascota.contains("    fun ladrar(): String"));

        let animal = content_of(&files, "Animal.kt");
        assert!(animal.contains("abstract class Animal {"));
        assert!(animal.contains("    protected var patas: Int = 0"));
        assert!(animal.contains("    abstract fun mover()"));

        let perro = content_of(&files, "Perro.kt");
        assert!(perro.contains("class Perro : Animal(), Mascota {"));
        assert!(perro.contains("    private var nombre: String = \"\""));
        assert!(perro.contains("    companion object {"));
        assert!(perro.contains("        public val ESPECIE: String = \"\""));
        assert!(perro.contains("        fun main() {"));
        assert!(perro.contains("    override fun ladrar(): String {"));
        assert!(perro.contains("        TODO(\"Implementar\")"));
        assert!(perro.contains("    override fun mover() {"));
    }

    #[test]
    fn kotlin_overrides_interface_properties() {
        let mut carnet = base_class("Carnet", ClassKind::Interface, false);
        carnet
            .fields
            .push(field("codigo", "String", Visibility::Public));
        let mut socio = base_class("Socio", ClassKind::Class, false);
        socio.implements.push("Carnet".to_string());

        let files = generate(&[carnet, socio], "kotlin").expect("generación kotlin");
        let socio = content_of(&files, "Socio.kt");
        assert!(socio.contains("    override val codigo: String = \"\""));
    }

    #[test]
    fn python_generates_abc_init_overrides_and_main_guard() {
        let mut classes = zoo();
        {
            let perro = classes
                .iter_mut()
                .find(|class| class.name == "Perro")
                .unwrap();
            let mut main = method("main", "void", vec![]);
            main.is_static = true;
            perro.methods.push(main);
        }

        let files = generate(&classes, "python").expect("generación python");
        assert_eq!(
            files
                .iter()
                .map(|file| file.relative.clone())
                .collect::<Vec<_>>(),
            vec!["mascota.py", "animal.py", "perro.py"]
        );

        let mascota = content_of(&files, "mascota.py");
        assert!(mascota.contains("from abc import ABC, abstractmethod"));
        assert!(mascota.contains("class Mascota(ABC):"));
        assert!(mascota.contains("    @abstractmethod\n    def ladrar(self) -> str:"));

        let perro = content_of(&files, "perro.py");
        assert!(perro.contains("from animal import Animal\nfrom mascota import Mascota"));
        assert!(perro.contains("class Perro(Animal, Mascota):"));
        assert!(perro.contains("    def __init__(self, nombre: str):"));
        assert!(perro.contains("        self.__nombre = nombre"));
        assert!(perro.contains("    def saludar(self, texto: str) -> None:"));
        assert!(perro.contains("        raise NotImplementedError(\"TODO\")"));
        assert!(perro.contains("    def ladrar(self) -> str:"));
        assert!(perro.contains("    def mover(self) -> None:"));
        assert!(perro.contains("def main() -> None:"));
        assert!(perro.contains("if __name__ == \"__main__\":\n    main()"));
    }

    #[test]
    fn python_uses_snake_case_file_names() {
        let figura = base_class("FiguraGeometrica", ClassKind::Class, false);
        let files = generate(&[figura], "python").expect("generación python");
        assert_eq!(files[0].relative, "figura_geometrica.py");
        assert_eq!(snake_case("ABCModel"), "abc_model");
        assert_eq!(snake_case("Perro"), "perro");
    }

    #[test]
    fn maps_types_per_language() {
        let mut clase = base_class("Bolsa", ClassKind::Class, false);
        clase
            .fields
            .push(field("elementos", "int[]", Visibility::Private));
        let files = generate(&[clase.clone()], "kotlin").expect("kotlin");
        assert!(content_of(&files, "Bolsa.kt").contains("List<Int> = emptyList()"));
        let files = generate(&[clase], "python").expect("python");
        assert!(content_of(&files, "bolsa.py").contains("def __init__(self, elementos: list):"));
    }

    #[test]
    fn rejects_invalid_designs() {
        assert!(generate(&[], "java").is_err());

        let mut classes = zoo();
        classes.push(base_class("Perro", ClassKind::Class, false));
        assert!(generate(&classes, "java")
            .unwrap_err()
            .contains("dos clases llamadas"));

        let mut invalid = base_class("1Perro", ClassKind::Class, false);
        invalid.name = "1Perro".to_string();
        assert!(generate(&[invalid], "java").is_err());

        let mut mascota = base_class("Mascota", ClassKind::Interface, false);
        let mut perro = base_class("Perro", ClassKind::Class, false);
        perro.extends.push("Mascota".to_string());
        assert!(generate(&[mascota.clone(), perro], "java")
            .unwrap_err()
            .contains("usa la relación «implementa»"));

        let mut perro = base_class("Perro", ClassKind::Class, false);
        perro.implements.push("Mascota".to_string());
        mascota.kind = ClassKind::Class;
        assert!(generate(&[mascota, perro], "java").is_err());

        let mut a = base_class("A", ClassKind::Class, false);
        let mut b = base_class("B", ClassKind::Class, false);
        a.extends.push("B".to_string());
        b.extends.push("A".to_string());
        assert!(generate(&[a, b], "java").unwrap_err().contains("ciclo"));

        let mut constructor = base_class("Perro", ClassKind::Class, false);
        let mut init = method("Perro", "void", vec![]);
        init.is_constructor = true;
        constructor.methods.push(init);
        assert!(generate(&[constructor], "java")
            .unwrap_err()
            .contains("constructores automáticamente"));

        assert!(generate(&zoo(), "ruby")
            .unwrap_err()
            .contains("No hay generador"));
    }

    #[test]
    fn java_and_kotlin_reject_multiple_inheritance_and_missing_types() {
        let a = base_class("A", ClassKind::Class, false);
        let b = base_class("B", ClassKind::Class, false);
        let mut c = base_class("C", ClassKind::Class, false);
        c.extends.push("A".to_string());
        c.extends.push("B".to_string());
        assert!(generate(&[a.clone(), b.clone(), c.clone()], "java").is_err());
        assert!(generate(&[a.clone(), b.clone(), c.clone()], "kotlin").is_err());
        assert!(generate(&[a, b, c], "python").is_ok());

        let mut sin_tipo = base_class("D", ClassKind::Class, false);
        sin_tipo.fields.push(field("x", "", Visibility::Public));
        assert!(generate(&[sin_tipo.clone()], "java")
            .unwrap_err()
            .contains("necesita un tipo"));
        assert!(generate(&[sin_tipo], "python").is_ok());
    }

    #[test]
    fn java_generates_only_the_missing_overload() {
        let mut pintable = base_class("Pintable", ClassKind::Interface, false);
        pintable
            .methods
            .push(method("pintar", "void", vec![("texto", "String")]));
        pintable
            .methods
            .push(method("pintar", "void", vec![("veces", "int")]));
        let mut cuadro = base_class("Cuadro", ClassKind::Class, false);
        cuadro.implements.push("Pintable".to_string());
        cuadro
            .methods
            .push(method("pintar", "void", vec![("texto", "String")]));

        let files = generate(&[pintable, cuadro], "java").expect("generación java");
        let cuadro = content_of(&files, "Cuadro.java");
        assert_eq!(cuadro.matches("pintar(String texto)").count(), 1);
        assert!(cuadro.contains("pintar(int veces)"));
    }

    fn write_all(root: &std::path::Path, files: &[GeneratedFile]) {
        std::fs::create_dir_all(root).unwrap();
        for file in files {
            std::fs::write(root.join(&file.relative), &file.content).unwrap();
        }
    }

    fn zoo_with_main() -> Vec<ClassModel> {
        let mut classes = zoo();
        let perro = classes
            .iter_mut()
            .find(|class| class.name == "Perro")
            .unwrap();
        let mut main = method("main", "void", vec![]);
        main.is_static = true;
        perro.methods.push(main);
        classes
    }

    #[test]
    fn generated_code_compiles_with_the_real_toolchains() {
        let classes = zoo_with_main();
        let base = std::env::temp_dir().join(format!("idecode-generador-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        for (language, extension) in [("java", "X.java"), ("kotlin", "X.kt"), ("python", "x.py")] {
            let Some(adapter) = crate::core::registry::for_extension(extension) else {
                continue;
            };
            let tool = match language {
                "java" => "javac",
                "kotlin" => "kotlinc",
                _ => "python3",
            };
            if std::process::Command::new(tool)
                .arg(if language == "python" {
                    "--version"
                } else {
                    "-version"
                })
                .output()
                .is_err()
            {
                continue;
            }
            let root = base.join(language);
            let files = generate(&classes, language).expect(language);
            write_all(&root, &files);
            let project = crate::core::project::Project::load(&root, adapter.extensions()).unwrap();
            let build = adapter.compile(&project).unwrap();
            assert!(
                build.success,
                "el esqueleto {language} no compila: {:?}",
                build.diagnostics
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}
