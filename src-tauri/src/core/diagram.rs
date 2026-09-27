use std::collections::{HashMap, HashSet};

use super::model::{ClassKind, ClassModel, MethodModel};

pub fn to_mermaid(classes: &[ClassModel]) -> String {
    let mut output = String::from("classDiagram\n    direction TB\n");

    if classes.is_empty() {
        output.push_str("    class Proyecto\n");
        return output;
    }

    for class in classes {
        render_class(class, &mut output);
    }
    let resolver = Resolver::new(classes);
    let mut linked: HashSet<(String, String)> = HashSet::new();

    for class in classes {
        for base in &class.extends {
            if let Some(target) = resolver.label_for(base, class) {
                output.push_str(&format!("    {target} <|-- {}\n", sanitize(&class.name)));
                linked.insert((class.name.clone(), target));
            }
        }
        for interface in &class.implements {
            if let Some(target) = resolver.label_for(interface, class) {
                output.push_str(&format!("    {target} <|.. {}\n", sanitize(&class.name)));
                linked.insert((class.name.clone(), target));
            }
        }
        for field in &class.fields {
            for target_name in &field.targets {
                let Some(target) = resolver.resolve(target_name, class) else {
                    continue;
                };
                let arrow = if field.multiplicity.is_aggregate() {
                    "o--"
                } else {
                    "-->"
                };
                output.push_str(&format!(
                    "    {} \"1\" {} \"{}\" {} : {}\n",
                    sanitize(&class.name),
                    arrow,
                    field.multiplicity.label(),
                    sanitize(&target.name),
                    sanitize(&field.name)
                ));
                linked.insert((class.name.clone(), target.name.clone()));
            }
        }
    }

    for class in classes {
        for usage in &class.uses {
            let Some(target) = resolver.dependency(usage, class) else {
                continue;
            };
            if target == class.name {
                continue;
            }
            if linked.insert((class.name.clone(), target.clone())) {
                output.push_str(&format!(
                    "    {} ..> {}\n",
                    sanitize(&class.name),
                    sanitize(&target)
                ));
            }
        }
    }

    output
}

struct Resolver<'a> {
    by_simple_name: HashMap<&'a str, Vec<&'a ClassModel>>,
    by_name: HashMap<&'a str, &'a ClassModel>,
}

impl<'a> Resolver<'a> {
    fn new(classes: &'a [ClassModel]) -> Self {
        let mut by_simple_name: HashMap<&'a str, Vec<&'a ClassModel>> = HashMap::new();
        let mut by_name: HashMap<&'a str, &'a ClassModel> = HashMap::new();
        for class in classes {
            by_simple_name
                .entry(class.simple_name.as_str())
                .or_default()
                .push(class);
            by_name.insert(class.name.as_str(), class);
        }
        Resolver {
            by_simple_name,
            by_name,
        }
    }

    fn resolve(&self, name: &str, from: &ClassModel) -> Option<&'a ClassModel> {
        if let Some(candidates) = self.by_simple_name.get(name) {
            if candidates.len() == 1 {
                return Some(candidates[0]);
            }

            let same_file: Vec<&&ClassModel> = candidates
                .iter()
                .filter(|candidate| candidate.file == from.file)
                .collect();
            if same_file.len() == 1 {
                return Some(*same_file[0]);
            }

            let same_package: Vec<&&ClassModel> = candidates
                .iter()
                .filter(|candidate| candidate.package == from.package)
                .collect();
            if same_package.len() == 1 {
                return Some(*same_package[0]);
            }
        }

        self.by_name
            .get(name)
            .copied()
            .filter(|candidate| candidate.name == name)
    }

    fn dependency(&self, name: &str, from: &ClassModel) -> Option<String> {
        if let Some(found) = self.resolve(name, from) {
            return Some(found.name.clone());
        }
        self.by_name.values().find_map(|candidate| {
            let last = candidate.name.rsplit('.').next().unwrap_or(&candidate.name);
            (last == name).then(|| candidate.name.clone())
        })
    }

    fn label_for(&self, raw: &str, from: &ClassModel) -> Option<String> {
        let simple_name = simple_name_of(raw);
        if simple_name.is_empty() {
            return None;
        }
        self.resolve(&simple_name, from)
            .map(|candidate| candidate.name.clone())
            .or_else(|| {
                self.resolve(raw, from)
                    .map(|candidate| candidate.name.clone())
            })
            .or(Some(simple_name))
    }
}

fn render_class(class: &ClassModel, output: &mut String) {
    output.push_str(&format!(
        "    class {}{} {{\n",
        sanitize(&class.name),
        generics(&class.type_parameters)
    ));
    if let Some(annotation) = annotation_of(class) {
        output.push_str(&format!("        {annotation}\n"));
    }
    for constant in &class.enum_constants {
        output.push_str(&format!("        {}\n", sanitize(&constant.name)));
    }
    for field in &class.fields {
        output.push_str(&format!(
            "        {}{}{}{}\n",
            field.visibility.marker(),
            field_type(&field.ty),
            field.name,
            if field.is_static { "$" } else { "" }
        ));
    }
    for method in &class.methods {
        output.push_str(&format!("        {}\n", method_line(method)));
    }
    for function in &class.functions {
        output.push_str(&format!("        {}\n", function_line(function)));
    }
    output.push_str("    }\n");
}

fn annotation_of(class: &ClassModel) -> Option<&'static str> {
    match class.kind {
        ClassKind::Interface => Some("<<interface>>"),
        ClassKind::Enum => Some("<<enumeration>>"),
        ClassKind::Annotation => Some("<<annotation>>"),
        ClassKind::Record => Some("<<record>>"),
        ClassKind::Module => Some("<<module>>"),
        ClassKind::Class if class.is_abstract => Some("<<abstract>>"),
        ClassKind::Class => None,
    }
}

fn function_line(function: &crate::core::model::FunctionModel) -> String {
    let parameters = parameter_list(&function.parameters);
    let prefix = if function.is_async { "async " } else { "" };
    match function.return_ty.trim() {
        "" => format!("{prefix}{}({parameters})", sanitize(&function.name)),
        return_ty => format!(
            "{prefix}{} {}({parameters})",
            mermaid_type(return_ty),
            sanitize(&function.name)
        ),
    }
}

fn parameter_list(parameters: &[crate::core::model::ParameterModel]) -> String {
    parameters
        .iter()
        .map(|parameter| {
            if parameter.ty.trim().is_empty() {
                parameter.name.clone()
            } else {
                format!("{} {}", mermaid_type(&parameter.ty), parameter.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn method_line(method: &MethodModel) -> String {
    let parameters = parameter_list(&method.parameters);
    let type_parameters = if method.type_parameters.is_empty() {
        String::new()
    } else {
        format!("{} ", generics(&method.type_parameters))
    };
    let throws = if method.throws.is_empty() {
        String::new()
    } else {
        format!(" throws {}", method.throws.join(", "))
    };
    let classifier = if method.is_static {
        "$"
    } else if method.is_abstract {
        "*"
    } else {
        ""
    };
    let prefix = if method.is_async { "async " } else { "" };

    if method.is_constructor {
        format!(
            "{}{}{}{}({}){}{}",
            method.visibility.marker(),
            prefix,
            type_parameters,
            sanitize(&method.name),
            parameters,
            throws,
            classifier
        )
    } else {
        format!(
            "{}{}{}{}{}({}){}{}",
            method.visibility.marker(),
            prefix,
            type_parameters,
            field_type(&method.return_ty),
            sanitize(&method.name),
            parameters,
            throws,
            classifier
        )
    }
}

fn generics(parameters: &[String]) -> String {
    if parameters.is_empty() {
        String::new()
    } else {
        format!("~{}~", parameters.join(","))
    }
}

fn mermaid_type(ty: &str) -> String {
    ty.trim().replace(['<', '>', '[', ']'], "~")
}

fn field_type(ty: &str) -> String {
    if ty.trim().is_empty() {
        String::new()
    } else {
        format!("{} ", mermaid_type(ty))
    }
}

fn sanitize(name: &str) -> String {
    name.trim().replace(['\n', '\r', '\t', ' '], "")
}

fn simple_name_of(ty: &str) -> String {
    let base = ty.split('<').next().unwrap_or(ty);
    let base = base.trim().trim_end_matches("[]").trim();
    base.rsplit('.').next().unwrap_or(base).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        ClassKind, ClassModel, EnumConstantModel, FieldModel, MethodModel, Multiplicity,
        ParameterModel, Visibility,
    };

    fn base_class(name: &str) -> ClassModel {
        ClassModel {
            name: name.into(),
            simple_name: name.into(),
            outer: None,
            kind: ClassKind::Class,
            package: None,
            is_abstract: false,
            type_parameters: vec![],
            extends: vec![],
            implements: vec![],
            enum_constants: vec![],
            fields: vec![],
            methods: vec![],
            functions: vec![],
            uses: vec![],
            line: 1,
            file: format!("{name}.java"),
        }
    }

    fn method(name: &str, return_ty: &str) -> MethodModel {
        MethodModel {
            name: name.into(),
            return_ty: return_ty.into(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_constructor: false,
            is_async: false,
            type_parameters: vec![],
            throws: vec![],
            parameters: vec![],
            line: 1,
        }
    }

    fn field(name: &str, ty: &str) -> FieldModel {
        FieldModel {
            name: name.into(),
            ty: ty.into(),
            targets: vec![],
            multiplicity: Multiplicity::One,
            visibility: Visibility::Private,
            is_static: false,
            is_final: false,
            line: 1,
        }
    }

    fn association(
        name: &str,
        ty: &str,
        targets: &[&str],
        multiplicity: Multiplicity,
    ) -> FieldModel {
        let mut model = field(name, ty);
        model.targets = targets.iter().map(|value| (*value).to_string()).collect();
        model.multiplicity = multiplicity;
        model
    }

    #[test]
    fn declares_classes_and_members() {
        let mut animal = base_class("Animal");
        animal.fields.push(field("nombre", "String"));
        animal.methods.push(method("hablar", "void"));

        let mermaid = to_mermaid(&[animal]);
        assert!(mermaid.starts_with("classDiagram"));
        assert!(mermaid.contains("class Animal {"));
        assert!(mermaid.contains("-String nombre"));
        assert!(mermaid.contains("+void hablar()"));
    }

    #[test]
    fn shows_abstract_classes_enums_and_constants() {
        let mut animal = base_class("Animal");
        animal.is_abstract = true;
        let mut color = base_class("Color");
        color.kind = ClassKind::Enum;
        color.enum_constants = vec![
            EnumConstantModel {
                name: "ROJO".into(),
                line: 3,
            },
            EnumConstantModel {
                name: "VERDE".into(),
                line: 4,
            },
        ];

        let mermaid = to_mermaid(&[animal, color]);
        assert!(mermaid.contains("<<abstract>>"));
        assert!(mermaid.contains("<<enumeration>>"));
        assert!(mermaid.contains("\n        ROJO\n"));
        assert!(mermaid.contains("\n        VERDE\n"));
    }

    #[test]
    fn marks_records() {
        let mut punto = base_class("Punto");
        punto.kind = ClassKind::Record;
        punto.fields.push(field("x", "int"));
        assert!(to_mermaid(&[punto]).contains("<<record>>"));
    }

    #[test]
    fn draws_inheritance_and_interface_relations() {
        let mut perro = base_class("Perro");
        perro.extends = vec!["Animal".into()];
        perro.implements = vec!["Serializable".into()];

        let mermaid = to_mermaid(&[perro]);
        assert!(mermaid.contains("Animal <|-- Perro"));
        assert!(mermaid.contains("Serializable <|.. Perro"));
    }

    #[test]
    fn draws_every_superinterface_of_an_interface() {
        let mut lectura = base_class("Lectura");
        lectura.kind = ClassKind::Interface;
        lectura.extends = vec!["Cerrable".into(), "AutoCloseable".into()];

        let mermaid = to_mermaid(&[lectura]);
        assert!(mermaid.contains("Cerrable <|-- Lectura"));
        assert!(mermaid.contains("AutoCloseable <|-- Lectura"));
    }

    #[test]
    fn adds_multiplicity_and_roles_to_associations() {
        let mut veterinario = base_class("Veterinario");
        veterinario.fields.push(association(
            "pacientes",
            "List<Animal>",
            &["Animal"],
            Multiplicity::ZeroOrMany,
        ));
        let mut equipo = base_class("Equipo");
        equipo.fields.push(association(
            "capitan",
            "Jugador",
            &["Jugador"],
            Multiplicity::One,
        ));
        equipo.fields.push(association(
            "suplente",
            "Optional<Jugador>",
            &["Jugador"],
            Multiplicity::ZeroOrOne,
        ));
        equipo.fields.push(association(
            "reservas",
            "Jugador[]",
            &["Jugador"],
            Multiplicity::ZeroOrMany,
        ));
        let jugador = base_class("Jugador");
        let animal = base_class("Animal");

        let mermaid = to_mermaid(&[veterinario, equipo, jugador, animal]);
        assert!(mermaid.contains("Veterinario \"1\" o-- \"0..*\" Animal : pacientes"));
        assert!(mermaid.contains("Equipo \"1\" --> \"1\" Jugador : capitan"));
        assert!(mermaid.contains("Equipo \"1\" --> \"0..1\" Jugador : suplente"));
        assert!(mermaid.contains("Equipo \"1\" o-- \"0..*\" Jugador : reservas"));
    }

    #[test]
    fn draws_the_association_target_of_a_container_field() {
        let mut registro = base_class("Registro");
        registro.fields.push(association(
            "duenos",
            "Map<String, List<Dueno>>",
            &["Dueno"],
            Multiplicity::ZeroOrMany,
        ));
        let dueno = base_class("Dueno");

        let mermaid = to_mermaid(&[registro, dueno]);
        assert!(mermaid.contains("Registro \"1\" o-- \"0..*\" Dueno : duenos"));
        assert!(!mermaid.contains("Registro ..> Dueno"));
    }

    #[test]
    fn draws_uses_as_dashed_dependencies() {
        let mut main = base_class("Main");
        main.uses = vec!["Mascota".into(), "Perro".into()];
        let mut mascota = base_class("Mascota");
        mascota.kind = ClassKind::Interface;

        let mermaid = to_mermaid(&[main, base_class("Perro"), mascota]);
        assert!(mermaid.contains("Main ..> Perro"));
        assert!(mermaid.contains("Main ..> Mascota"));
        assert_eq!(mermaid.matches("Main ..> Perro").count(), 1);
        assert!(!mermaid.contains("Main --> Perro"));
    }

    #[test]
    fn ignores_types_that_are_not_project_classes() {
        let mut uso = base_class("Uso");
        uso.uses = vec!["String".into(), "int".into(), "java".into(), "List".into()];

        let mermaid = to_mermaid(&[uso]);
        assert!(!mermaid.contains("..>"));
    }

    #[test]
    fn a_field_association_is_not_repeated_as_a_dependency() {
        let mut veterinario = base_class("Veterinario");
        veterinario.fields.push(association(
            "pacientes",
            "List<Animal>",
            &["Animal"],
            Multiplicity::ZeroOrMany,
        ));
        veterinario.uses = vec!["Animal".into(), "List".into()];

        let mermaid = to_mermaid(&[veterinario, base_class("Animal")]);
        assert!(mermaid.contains("Veterinario \"1\" o-- \"0..*\" Animal : pacientes"));
        assert!(!mermaid.contains("Veterinario ..> Animal"));
    }

    #[test]
    fn strips_generics_and_packages_from_relation_targets() {
        let mut saludable = base_class("Saludable");
        saludable.implements = vec!["Comparable<Saludable>".into()];
        let mut puerta = base_class("Puerta");
        puerta.extends = vec!["java.io.Closeable".into()];

        let mermaid = to_mermaid(&[saludable, puerta]);
        assert!(mermaid.contains("Comparable <|.. Saludable"));
        assert!(mermaid.contains("Closeable <|-- Puerta"));
        assert!(!mermaid.contains("Comparable<Saludable>"));
        assert!(!mermaid.contains("java.io.Closeable"));
    }

    #[test]
    fn does_not_draw_a_class_depending_on_itself() {
        let mut solo = base_class("Solo");
        solo.uses = vec!["Solo".into()];
        assert!(!to_mermaid(&[solo]).contains("Solo ..> Solo"));
    }

    #[test]
    fn draws_a_field_that_points_back_to_its_own_class() {
        let mut nodo = base_class("Nodo");
        nodo.fields
            .push(association("padre", "Nodo", &["Nodo"], Multiplicity::One));
        nodo.fields.push(association(
            "hijos",
            "List<Nodo>",
            &["Nodo"],
            Multiplicity::ZeroOrMany,
        ));

        let mermaid = to_mermaid(&[nodo]);
        assert!(mermaid.contains("Nodo \"1\" --> \"1\" Nodo : padre"));
        assert!(mermaid.contains("Nodo \"1\" o-- \"0..*\" Nodo : hijos"));
        assert!(!mermaid.contains("Nodo ..> Nodo"));
    }

    #[test]
    fn uses_the_qualified_name_of_nested_classes() {
        let mut outer = base_class("Outer");
        let mut inner = base_class("Outer.Inner");
        inner.simple_name = "Inner".into();
        inner.outer = Some("Outer".into());
        outer.uses = vec!["Inner".into()];

        let mermaid = to_mermaid(&[outer, inner]);
        assert!(mermaid.contains("class Outer.Inner {"));
        assert!(mermaid.contains("Outer ..> Outer.Inner"));
    }

    #[test]
    fn resolves_a_repeated_simple_name_using_the_nearest_candidate() {
        let mut consumer = base_class("Consumer");
        consumer.uses = vec!["Helper".into()];

        let mut same_file = base_class("Helper");
        same_file.file = "Consumer.java".into();

        let mut elsewhere = base_class("Helper");
        elsewhere.name = "otro.Helper".into();
        elsewhere.outer = Some("otro".into());
        elsewhere.file = "otro/Helper.java".into();

        let mermaid = to_mermaid(&[consumer, same_file, elsewhere]);
        assert!(mermaid.contains("Consumer ..> Helper"));
        assert!(!mermaid.contains("Consumer ..> otro.Helper"));
    }

    #[test]
    fn renders_generics_and_throws() {
        let mut caja = base_class("Caja");
        caja.type_parameters = vec!["T".into()];
        let mut transformar = method("transformar", "T");
        transformar.type_parameters = vec!["U".into()];
        transformar.parameters.push(ParameterModel {
            name: "valor".into(),
            ty: "U".into(),
        });
        transformar.throws = vec!["IOException".into()];
        caja.methods.push(transformar);

        let mermaid = to_mermaid(&[caja]);
        assert!(mermaid.contains("class Caja~T~ {"));
        assert!(mermaid.contains("+~U~ T transformar(U valor) throws IOException"));
    }

    #[test]
    fn marks_interface_and_static_members() {
        let mut forma = base_class("Forma");
        forma.kind = ClassKind::Interface;
        let mut pi = field("PI", "double");
        pi.visibility = Visibility::Public;
        pi.is_static = true;
        pi.is_final = true;
        forma.fields.push(pi);
        let mut area = method("area", "double");
        area.is_abstract = true;
        area.parameters.push(ParameterModel {
            name: "escala".into(),
            ty: "double".into(),
        });
        forma.methods.push(area);

        let mermaid = to_mermaid(&[forma]);
        assert!(mermaid.contains("<<interface>>"));
        assert!(mermaid.contains("+double PI$"));
        assert!(mermaid.contains("+double area(double escala)*"));
    }

    #[test]
    fn escapes_generics_and_empty_projects() {
        let mut perro = base_class("Perro");
        perro.fields.push(field("juguete", "List<Juguete>"));

        let mermaid = to_mermaid(&[perro]);
        assert!(mermaid.contains("List~Juguete~"));
        assert!(!mermaid.contains("List<Juguete>"));

        assert!(to_mermaid(&[]).contains("class Proyecto"));
    }

    #[test]
    fn renders_free_functions_in_a_module() {
        let mut module = base_class("app");
        module.kind = ClassKind::Module;
        module.file = "app.py".into();
        module.functions = vec![
            crate::core::model::FunctionModel {
                name: "saludar".into(),
                return_ty: String::new(),
                is_async: false,
                parameters: vec![ParameterModel {
                    name: "nombre".into(),
                    ty: "str".into(),
                }],
                line: 3,
            },
            crate::core::model::FunctionModel {
                name: "cargar".into(),
                return_ty: "int".into(),
                is_async: true,
                parameters: vec![],
                line: 7,
            },
        ];

        let mermaid = to_mermaid(&[module]);
        assert!(mermaid.contains("<<module>>"));
        assert!(mermaid.contains("saludar(str nombre)"));
        assert!(mermaid.contains("async int cargar()"));
    }

    #[test]
    fn resolves_a_dotted_python_dependency_to_its_local_name() {
        let mut main = base_class("main");
        main.kind = ClassKind::Module;
        main.file = "main.py".into();
        main.uses = vec!["animales.perro".into()];

        let mut perro = base_class("animales.perro");
        perro.kind = ClassKind::Module;
        perro.file = "animales/perro.py".into();

        let mermaid = to_mermaid(&[main, perro]);
        assert!(mermaid.contains("main ..> animales.perro"));
    }

    #[test]
    fn renders_an_untyped_attribute_without_a_dangling_space() {
        let mut perro = base_class("Perro");
        perro.fields.push(field("vidas", ""));
        perro.fields.push(field("juguetes", "list[str]"));

        let mermaid = to_mermaid(&[perro]);
        assert!(mermaid.contains("\n        -vidas\n"));
        assert!(mermaid.contains("list~str~ juguetes"));
        assert!(!mermaid.contains("list[str]"));
    }
}
