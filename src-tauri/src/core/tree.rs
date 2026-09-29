//! Helpers compartidos de tree-sitter para los adaptadores de lenguaje.
//!
//! Todos los adaptadores recorren árboles sintácticos de la misma forma;
//! estas funciones evitan reimplementarlas (y que diverjan) en cada lenguaje.
//!
//! Para añadir un lenguaje nuevo, usa estas funciones en tu adaptador en vez
//! de copiarlas: `text`, `find_child`, `find_named_child`,
//! `find_named_child_of_kinds`, `find_not_kind`, `first_named_text`,
//! `has_child`, `has_token` y `visibility_with`.

use tree_sitter::Node;

use super::model::Visibility;

/// Profundidad máxima al desenredar tipos genéricos anidados.
///
/// Evita desbordar la pila ante entradas patológicas como `List<List<…>>`
/// con miles de niveles. Pásala como límite en las recursiones de cada
/// adaptador (`collect_type_names` y similares).
pub const MAX_TYPE_DEPTH: usize = 32;

/// Texto recortado del nodo en la fuente original.
pub fn text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

/// Primer hijo (nombrado o no) del tipo indicado.
pub fn find_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

/// Primer hijo nombrado del tipo indicado.
pub fn find_named_child<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

/// Primer hijo nombrado cuyo tipo esté en la lista indicada.
pub fn find_named_child_of_kinds<'tree>(node: Node<'tree>, kinds: &[&str]) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

/// Primer hijo nombrado de un tipo distinto al indicado.
pub fn find_not_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() != kind);
    found
}

/// Texto del primer hijo nombrado, si existe.
pub fn first_named_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let first = node.named_children(&mut cursor).next();
    first.map(|child| text(child, source))
}

/// Indica si hay algún hijo (nombrado o no) del tipo indicado.
pub fn has_child(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|child| child.kind() == kind);
    found
}

/// Indica si hay un token anónimo del tipo indicado (p. ej. `async`).
pub fn has_token(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| !child.is_named() && child.kind() == kind);
    found
}

/// Visibilidad a partir de la primera regla que coincida, en orden.
///
/// Cada lenguaje define su tabla porque la semántica difiere: en Java la
/// ausencia de modificador significa paquete, mientras que en Kotlin
/// significa público y `internal` equivale a paquete.
pub fn visibility_with(
    modifiers: &[String],
    rules: &[(&str, Visibility)],
    default: Visibility,
) -> Visibility {
    for (marker, visibility) in rules {
        if modifiers.iter().any(|modifier| modifier == marker) {
            return *visibility;
        }
    }
    default
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn java_tree(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("gramática java");
        parser.parse(source, None).expect("árbol java")
    }

    #[test]
    fn finds_children_by_kind() {
        let tree = java_tree("public class Perro { private String nombre; }");
        let root = tree.root_node();
        let declaration =
            find_named_child(root, "class_declaration").expect("declaración de clase");
        assert!(find_child(declaration, "modifiers").is_some());
        assert!(find_named_child(declaration, "identifier").is_some());
        assert!(find_named_child_of_kinds(declaration, &["identifier", "missing"]).is_some());
        assert!(find_named_child(declaration, "inexistente").is_none());
        assert!(find_not_kind(declaration, "identifier").is_some());
        assert_eq!(
            first_named_text(
                declaration,
                b"public class Perro { private String nombre; }"
            ),
            Some("public".to_string())
        );
    }

    #[test]
    fn detects_tokens_and_children() {
        let tree = java_tree("public static void main() {}");
        let root = tree.root_node();
        assert!(has_child(root, "method_declaration"));
        assert!(!has_child(root, "class_declaration"));
        assert!(!has_token(root, "static"));
    }

    #[test]
    fn applies_visibility_rules_in_order() {
        let java = [
            ("public", Visibility::Public),
            ("protected", Visibility::Protected),
            ("private", Visibility::Private),
        ];
        assert_eq!(
            visibility_with(&["public".to_string()], &java, Visibility::Package),
            Visibility::Public
        );
        assert_eq!(
            visibility_with(&[], &java, Visibility::Package),
            Visibility::Package
        );
    }
}
