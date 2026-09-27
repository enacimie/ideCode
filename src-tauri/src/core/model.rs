use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    Record,
    Annotation,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Protected,
    Package,
    Private,
}

impl Visibility {
    pub fn marker(self) -> char {
        match self {
            Visibility::Public => '+',
            Visibility::Protected => '#',
            Visibility::Package => '~',
            Visibility::Private => '-',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Multiplicity {
    #[default]
    One,
    ZeroOrOne,
    ZeroOrMany,
}

impl Multiplicity {
    pub fn label(self) -> &'static str {
        match self {
            Multiplicity::One => "1",
            Multiplicity::ZeroOrOne => "0..1",
            Multiplicity::ZeroOrMany => "0..*",
        }
    }

    pub fn is_aggregate(self) -> bool {
        self == Multiplicity::ZeroOrMany
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterModel {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldModel {
    pub name: String,
    pub ty: String,
    pub targets: Vec<String>,
    pub multiplicity: Multiplicity,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_final: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumConstantModel {
    pub name: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodModel {
    pub name: String,
    pub return_ty: String,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_constructor: bool,
    pub is_async: bool,
    pub type_parameters: Vec<String>,
    pub throws: Vec<String>,
    pub parameters: Vec<ParameterModel>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionModel {
    pub name: String,
    pub return_ty: String,
    pub is_async: bool,
    pub parameters: Vec<ParameterModel>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassModel {
    pub name: String,
    pub simple_name: String,
    pub outer: Option<String>,
    pub kind: ClassKind,
    pub package: Option<String>,
    pub is_abstract: bool,
    pub type_parameters: Vec<String>,
    pub extends: Vec<String>,
    pub implements: Vec<String>,
    pub enum_constants: Vec<EnumConstantModel>,
    pub fields: Vec<FieldModel>,
    pub methods: Vec<MethodModel>,
    pub functions: Vec<FunctionModel>,
    pub uses: Vec<String>,
    pub line: u32,
    pub file: String,
}

impl ClassModel {
    pub fn qualified_name(&self) -> String {
        let nested = self.name.replace('.', "$");
        match &self.package {
            Some(package) if !package.is_empty() => format!("{package}.{nested}"),
            _ => nested,
        }
    }

    pub fn file_stem(&self) -> &str {
        self.name.split('.').next().unwrap_or(&self.name)
    }

    pub fn has_main(&self) -> bool {
        self.methods
            .iter()
            .any(|method| method.name == "main" && method.is_static)
    }
}

pub fn module_model(name: &str, file: &str) -> ClassModel {
    ClassModel {
        name: name.to_string(),
        simple_name: name.to_string(),
        outer: None,
        kind: ClassKind::Module,
        package: None,
        is_abstract: false,
        type_parameters: Vec::new(),
        extends: Vec::new(),
        implements: Vec::new(),
        enum_constants: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        functions: Vec::new(),
        uses: Vec::new(),
        line: 1,
        file: file.to_string(),
    }
}
