//! Grapl is a notation for graphs.
//!
//! ```grapl
//! {A, B, C}       fully connected
//! [A, B, C]       fully disconnected
//! (A, B, C)       directed, from earlier to later
//! A -> B -> C     a path, sugar for [(A, B), (B, C)]
//! G = {A, B}      a named graph, see `resolve`
//! ```
//!
//! The crate is split along the stages an expression goes through:
//!
//! - This module defines the syntax tree: [`Node`], [`Expr`], [`Stmt`], [`Program`].
//! - [`parse`] turns text into syntax, see [`Parse`].
//! - [`resolve`] replaces named graphs with their definitions, see [`Resolve`].
//! - [`graph`] gives the meaning of an expression as a [`Graph`] of nodes and
//!   edges. Two expressions are the same graph when their [`Expr::graph`]s are
//!   equal.
//! - [`normal`] builds the canonical compact expression for a [`Graph`], see
//!   [`Normalize`].

use std::fmt;

/// Nodes used as base indentifiers or to refer to other graphs.
///
/// Examples of nodes: `A`, `a`, `G1`...
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Node(String);

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Expressions describe a graph.
///
/// ```grapl
/// { A, B }
/// { A, [B, C] }
/// ( A, B )
/// A -> B -> C
/// ```
///
/// Only the three bracket forms exist in the algebra. `A -> B -> C` is sugar
/// for the path `[(A, B), (B, C)]` and is desugared by the parser.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Node(Node),
    /// `{A, B, C}`: an edge in both directions between every pair of members.
    ///
    /// This is the symmetric closure of [`Expr::Directed`], that is
    /// `{X, Y} = [(X, Y), (Y, X)]`, and so it is commutative.
    Connected(Vec<Expr>),
    /// `[A, B, C]`: adds no edges between its members, in either direction.
    ///
    /// Members with disjoint node sets become separate components; members
    /// sharing a node are joined through it. So `[N, {N, J}]` is the same
    /// graph as `{N, J}`.
    Disconnected(Vec<Expr>),
    /// `(A, B, C)`: an edge from every member to every *later* member.
    ///
    /// Order matters, so this is not commutative. Note that `(A, B, C)` has
    /// three edges `A->B`, `A->C`, `B->C`, it is a transitive tournament and
    /// not a path. For a path use `A -> B -> C`.
    Directed(Vec<Expr>),
}

impl Expr {
    /// Builds the path through the given expressions, in order.
    ///
    /// ```grapl
    /// A -> B -> C => [(A, B), (B, C)]
    /// ```
    pub fn path(mut exprs: Vec<Expr>) -> Expr {
        match exprs.len() {
            0 => Expr::Disconnected(vec![]),
            1 => exprs.remove(0),
            2 => Expr::Directed(exprs),
            _ => Expr::Disconnected(
                exprs
                    .windows(2)
                    .map(|w| Expr::Directed(w.to_vec()))
                    .collect(),
            ),
        }
    }

    /// Returns the members of a bracketed expression, or none for a node.
    pub fn members(&self) -> &[Expr] {
        match self {
            Expr::Node(_) => &[],
            Expr::Connected(exprs) | Expr::Disconnected(exprs) | Expr::Directed(exprs) => exprs,
        }
    }

    /// Returns true if the given node is anywhere inside this expression.
    pub fn contains(&self, node: &Node) -> bool {
        match self {
            Expr::Node(n) => node == n,
            _ => self.members().iter().any(|e| e.contains(node)),
        }
    }

    /// Returns true if any part of this expression is directed.
    pub fn is_directed(&self) -> bool {
        match self {
            Expr::Directed(_) => true,
            _ => self.members().iter().any(Expr::is_directed),
        }
    }

    /// Reverses the direction of every edge.
    ///
    /// Only [`Expr::Directed`] carries direction, so this just reverses the
    /// order of every directed expression.
    pub fn transpose(&self) -> Expr {
        let transposed = |exprs: &[Expr]| exprs.iter().map(Expr::transpose).collect();
        match self {
            Expr::Node(node) => Expr::Node(node.clone()),
            Expr::Connected(exprs) => Expr::Connected(transposed(exprs)),
            Expr::Disconnected(exprs) => Expr::Disconnected(transposed(exprs)),
            Expr::Directed(exprs) => {
                let mut exprs: Vec<Expr> = transposed(exprs);
                exprs.reverse();
                Expr::Directed(exprs)
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let joined = |exprs: &[Expr]| {
            exprs
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Expr::Node(node) => write!(f, "{}", node),
            Expr::Connected(exprs) => write!(f, "{{{}}}", joined(exprs)),
            Expr::Disconnected(exprs) => write!(f, "[{}]", joined(exprs)),
            Expr::Directed(exprs) => write!(f, "({})", joined(exprs)),
        }
    }
}

/// A statement is part of a sequence used to resolve other statements.
///
/// ```grapl
/// G1 = {A, B}
/// G2 = [C, D]
/// G  = {G1, G2}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt {
    Assign(Node, Expr),
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Assign(node, expr) => write!(f, "{} = {}", node, expr),
        }
    }
}

/// A program is a sequence of statements followed by a final graph expression.
///
/// ```grapl
/// G = {A, B}
/// {G, [C, D]}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    pub expr: Expr,
}

impl Program {
    pub fn new(stmts: Vec<Stmt>, expr: Expr) -> Self {
        Program { stmts, expr }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stmt in &self.stmts {
            writeln!(f, "{}", stmt)?;
        }
        write!(f, "{}", self.expr)
    }
}

// Test helpers shared by every module's tests. These are textually scoped so
// they must come before the module declarations.
#[cfg(test)]
macro_rules! node {
    ($id:ident) => {
        $crate::Node(stringify!($id).into())
    };
}

#[cfg(test)]
macro_rules! enode {
    ($id:ident) => {
        $crate::Expr::Node($crate::Node(stringify!($id).into()))
    };
}

pub mod parse;
pub use self::parse::Parse;

pub mod graph;
pub use self::graph::Graph;

mod normal;
pub use self::normal::Normalize;

pub mod resolve;
pub use self::resolve::Resolve;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn display_node() {
        assert_eq!(Node::parse("A").unwrap().to_string(), "A");
        assert_eq!(Node::parse("  G ").unwrap().to_string(), "G");
    }

    #[test]
    fn contains_expr() {
        assert!(!Expr::parse("{}").unwrap().contains(&node!(A)));
        assert!(!Expr::parse("[]").unwrap().contains(&node!(A)));
        assert!(!Expr::parse("[A]").unwrap().contains(&node!(B)));
        assert!(
            Expr::parse("{A, {B, [C, D]}, {E, F}}")
                .unwrap()
                .contains(&node!(C))
        );
        assert!(Expr::parse("(A, B)").unwrap().contains(&node!(B)));
    }

    #[test]
    fn is_directed_expr() {
        assert!(!Expr::parse("{A, [B, C]}").unwrap().is_directed());
        assert!(Expr::parse("{A, [B, (C, D)]}").unwrap().is_directed());
        assert!(Expr::parse("A -> B").unwrap().is_directed());
    }

    #[test]
    fn transpose_expr() {
        assert_eq!(
            Expr::parse("[(A, B), {C, D}]").unwrap().transpose(),
            Expr::parse("[(B, A), {C, D}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, (B, C))").unwrap().transpose(),
            Expr::parse("((C, B), A)").unwrap(),
        );
        assert_eq!(
            Expr::parse("{A, [B, C]}").unwrap().transpose(),
            Expr::parse("{A, [B, C]}").unwrap(),
        );
    }

    #[test]
    fn display_expr() {
        assert_eq!(
            Expr::parse("{  A, {B  ,  [C,D]}  }").unwrap().to_string(),
            "{A, {B, [C, D]}}"
        );
        assert_eq!(
            Expr::parse("( A,(B,  C) )").unwrap().to_string(),
            "(A, (B, C))"
        );
        assert_eq!(
            Expr::parse("A -> B -> C").unwrap().to_string(),
            "[(A, B), (B, C)]"
        );
    }

    #[test]
    fn display_stmt() {
        assert_eq!(
            Stmt::parse("  G={A,[B,C]}").unwrap().to_string(),
            "G = {A, [B, C]}"
        )
    }

    #[test]
    fn display_ret() {
        assert_eq!(Program::parse("  G=A B").unwrap().to_string(), "G = A\nB");
        assert_eq!(
            Program::parse("G = A\nH = {G, B}\n[H, C]")
                .unwrap()
                .to_string(),
            "G = A\nH = {G, B}\n[H, C]"
        );
    }
}
