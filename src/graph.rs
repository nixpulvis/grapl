//! The meaning of an expression.
//!
//! An expression denotes a [`Graph`]: a set of nodes and a set of directed
//! edges. This is computed directly from the syntax, without normalizing, so
//! it is the ground truth that [`Normalize`](crate::Normalize) must preserve.
//!
//! The two bracket operations of the algebra are [`Graph::overlay`] (`[]`) and
//! [`Graph::connect`] (`()`), with `{}` being connect in both directions. This
//! is Mokhov's algebra of graphs, so the usual laws hold:
//!
//! - overlay is associative, commutative and idempotent
//! - connect is associative and distributes over overlay
//! - the empty graph is the identity of both
//! - decomposition: `(X, Y, Z) = [(X, Y), (X, Z), (Y, Z)]`

use crate::{Expr, Node};
use std::collections::BTreeSet;

/// A graph as a set of nodes and a set of directed edges.
///
/// Undirected edges are stored as a pair of directed edges, so `{A, B}` has
/// both `(A, B)` and `(B, A)`. There are no self edges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Graph {
    nodes: BTreeSet<Node>,
    edges: BTreeSet<(Node, Node)>,
}

impl Graph {
    /// The graph with no nodes: `[]`.
    pub fn empty() -> Self {
        Graph::default()
    }

    /// The graph with a single node and no edges: `A`.
    pub fn node(node: Node) -> Self {
        Graph {
            nodes: BTreeSet::from([node]),
            edges: BTreeSet::new(),
        }
    }

    /// The **sorted** nodes of this graph.
    pub fn nodes(&self) -> &BTreeSet<Node> {
        &self.nodes
    }

    /// The **sorted** directed edges of this graph.
    pub fn edges(&self) -> &BTreeSet<(Node, Node)> {
        &self.edges
    }

    /// Union of both graphs: `[X, Y]`.
    pub fn overlay(mut self, other: Self) -> Self {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
        self
    }

    /// Union of both graphs plus an edge from every node of this graph to
    /// every node of the other: `(X, Y)`.
    pub fn connect(mut self, other: Self) -> Self {
        for a in &self.nodes {
            for b in &other.nodes {
                if a != b {
                    self.edges.insert((a.clone(), b.clone()));
                }
            }
        }
        self.overlay(other)
    }

    /// Reverses every edge.
    pub fn transpose(&self) -> Self {
        Graph {
            nodes: self.nodes.clone(),
            edges: self
                .edges
                .iter()
                .map(|(a, b)| (b.clone(), a.clone()))
                .collect(),
        }
    }

    /// Returns true if every node and edge of this graph is in the other.
    pub fn is_subgraph(&self, other: &Self) -> bool {
        self.nodes.is_subset(&other.nodes) && self.edges.is_subset(&other.edges)
    }
}

impl Expr {
    /// The graph this expression describes.
    pub fn graph(&self) -> Graph {
        let graphs = || self.members().iter().map(Expr::graph);
        match self {
            Expr::Node(node) => Graph::node(node.clone()),
            Expr::Disconnected(_) => graphs().fold(Graph::empty(), Graph::overlay),
            Expr::Directed(_) => graphs().fold(Graph::empty(), Graph::connect),
            // {X, Y} = [(X, Y), (Y, X)]
            Expr::Connected(_) => graphs().fold(Graph::empty(), |acc, g| {
                acc.clone().connect(g.clone()).overlay(g.connect(acc))
            }),
        }
    }

    /// Returns the **sorted** list of nodes for this expression.
    pub fn nodes(&self) -> Vec<Node> {
        self.graph().nodes.into_iter().collect()
    }

    /// Returns the **sorted** list of edges for this expression.
    ///
    /// Edges are directed pairs. An undirected edge `{A, B}` shows up as both
    /// `(A, B)` and `(B, A)`.
    pub fn edges(&self) -> Vec<(Node, Node)> {
        self.graph().edges.into_iter().collect()
    }
}

#[cfg(feature = "petgraph")]
impl From<&Graph> for petgraph::Graph<Node, ()> {
    fn from(graph: &Graph) -> Self {
        let mut pet = petgraph::Graph::new();
        let indices: std::collections::BTreeMap<_, _> = graph
            .nodes
            .iter()
            .map(|node| (node.clone(), pet.add_node(node.clone())))
            .collect();
        for (a, b) in &graph.edges {
            pet.add_edge(indices[a], indices[b], ());
        }
        pet
    }
}

#[cfg(feature = "petgraph")]
impl From<&Expr> for petgraph::Graph<Node, ()> {
    fn from(expr: &Expr) -> Self {
        (&expr.graph()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parse;
    use pretty_assertions::assert_eq;

    fn graph(src: &str) -> Graph {
        Expr::parse(src).unwrap().graph()
    }

    #[test]
    fn nodes_expr() {
        assert_eq!(
            Expr::parse("{A, [B, C], D}").unwrap().nodes(),
            vec![node!(A), node!(B), node!(C), node!(D)]
        );
        assert_eq!(
            Expr::parse("[{A,B},{B,C},{C,D},{D,A}]").unwrap().nodes(),
            vec![node!(A), node!(B), node!(C), node!(D)]
        );
    }

    #[test]
    fn edges_expr() {
        assert_eq!(
            Expr::parse("{A, [C, B], D}").unwrap().edges(),
            vec![
                (node!(A), node!(B)),
                (node!(A), node!(C)),
                (node!(A), node!(D)),
                (node!(B), node!(A)),
                (node!(B), node!(D)),
                (node!(C), node!(A)),
                (node!(C), node!(D)),
                (node!(D), node!(A)),
                (node!(D), node!(B)),
                (node!(D), node!(C)),
            ]
        );
        assert_eq!(
            Expr::parse("[{A,B}, {A,B}]").unwrap().edges(),
            vec![(node!(A), node!(B)), (node!(B), node!(A)),]
        );
        // No self edges.
        assert_eq!(Expr::parse("{A, A}").unwrap().edges(), vec![]);
        assert_eq!(Expr::parse("(A, A)").unwrap().edges(), vec![]);
    }

    #[test]
    fn edges_directed_expr() {
        assert_eq!(
            Expr::parse("(A, B, C)").unwrap().edges(),
            vec![
                (node!(A), node!(B)),
                (node!(A), node!(C)),
                (node!(B), node!(C)),
            ]
        );
        assert_eq!(
            Expr::parse("A -> B -> C").unwrap().edges(),
            vec![(node!(A), node!(B)), (node!(B), node!(C)),]
        );
        assert_eq!(
            Expr::parse("{A, (B, C)}").unwrap().edges(),
            vec![
                (node!(A), node!(B)),
                (node!(A), node!(C)),
                (node!(B), node!(A)),
                (node!(B), node!(C)),
                (node!(C), node!(A)),
            ]
        );
        assert_eq!(graph("[(A, B), (B, A)]"), graph("{A, B}"));
        assert_eq!(graph("(A, B, A)"), graph("{A, B}"));
    }

    #[test]
    fn transpose_graph() {
        assert_eq!(graph("(A, B, C)").transpose(), graph("(C, B, A)"));
        assert_eq!(graph("{A, [B, C]}").transpose(), graph("{A, [B, C]}"));
    }

    #[test]
    fn subgraph() {
        assert!(graph("[]").is_subgraph(&graph("A")));
        assert!(graph("A").is_subgraph(&graph("{A, B}")));
        assert!(graph("{A, B}").is_subgraph(&graph("{A, B, C}")));
        assert!(graph("(A, B)").is_subgraph(&graph("(A, C, B)")));
        assert!(!graph("(B, A)").is_subgraph(&graph("(A, C, B)")));
        assert!(!graph("{A, B}").is_subgraph(&graph("(A, B)")));
    }

    #[test]
    fn laws() {
        // Identity.
        assert_eq!(graph("[A, []]"), graph("A"));
        assert_eq!(graph("{A, []}"), graph("A"));
        assert_eq!(graph("(A, [])"), graph("A"));
        // Overlay is commutative and idempotent, connect is neither.
        assert_eq!(graph("[A, B]"), graph("[B, A]"));
        assert_eq!(graph("[{A, B}, {A, B}]"), graph("{A, B}"));
        assert_ne!(graph("(A, B)"), graph("(B, A)"));
        assert_eq!(graph("({A, B}, {A, B})"), graph("{A, B}"));
        // Both are associative.
        assert_eq!(graph("[[A, B], C]"), graph("[A, [B, C]]"));
        assert_eq!(graph("((A, B), C)"), graph("(A, (B, C))"));
        // Connect distributes over overlay.
        assert_eq!(graph("(A, [B, C])"), graph("[(A, B), (A, C)]"));
        assert_eq!(graph("([A, B], C)"), graph("[(A, C), (B, C)]"));
        // Decomposition.
        assert_eq!(graph("(A, B, C)"), graph("[(A, B), (A, C), (B, C)]"));
        assert_eq!(graph("{A, B, C}"), graph("[{A, B}, {A, C}, {B, C}]"));
        // Absorption, which is what dedup relies on.
        assert_eq!(graph("[A, {A, B}]"), graph("{A, B}"));
        assert_eq!(graph("[N, I, {N, J}]"), graph("[I, {N, J}]"));
    }
}
