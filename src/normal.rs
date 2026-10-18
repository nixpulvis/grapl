//! Normal forms.
//!
//! The normal form of an expression is computed from its [`Graph`] alone, so
//! two expressions have the same normal form exactly when they describe the
//! same graph, and normalizing twice is the same as normalizing once. It is a
//! disconnected list of
//!
//! - nodes without edges,
//! - cliques `{...}` covering every edge that goes both ways, and
//! - transitive tournaments `(...)` covering every edge that goes one way,
//!
//! with nodes and members in sorted order. A single member stands on its own
//! and the empty graph is `[]`. The laws of the algebra all fall out of this:
//!
//! ```grapl
//! {{A, B}, C}              => {A, B, C}                flattening
//! {A, []}                  => A                        identity
//! {S, [A, B]}              => [{A, S}, {B, S}]         distributivity
//! {A, (B, C)}              => [{A, B}, {A, C}, (B, C)]  {X, Y} = [(X, Y), (Y, X)]
//! [A, {A, B}]              => {A, B}                   absorption
//! [{A, B}, {B, C}, {A, C}] => {A, B, C}
//! (A, B, A)                => {A, B}
//! ```
//!
//! The cover is built greedily, so it isn't always the smallest possible one.

use crate::{Expr, Graph, Node, Program, Stmt};
use std::collections::{BTreeSet, HashSet};

pub trait Normalize: Sized {
    fn normalize(&self) -> Self;
}

impl Normalize for Expr {
    fn normalize(&self) -> Self {
        Expr::from(&self.graph())
    }
}

impl From<&Graph> for Expr {
    fn from(graph: &Graph) -> Expr {
        Cover::new(graph).into_expr()
    }
}

// Builds a compact expression for a graph by covering its edges.
struct Cover<'g> {
    graph: &'g Graph,
    edges: HashSet<(&'g Node, &'g Node)>,
    uncovered: BTreeSet<(&'g Node, &'g Node)>,
}

impl<'g> Cover<'g> {
    fn new(graph: &'g Graph) -> Self {
        let edges: HashSet<_> = graph.edges().iter().map(|(a, b)| (a, b)).collect();
        let uncovered = edges.iter().copied().collect();
        Cover {
            graph,
            edges,
            uncovered,
        }
    }

    fn has(&self, a: &Node, b: &Node) -> bool {
        self.edges.contains(&(a, b))
    }

    fn into_expr(mut self) -> Expr {
        let mut members = vec![];

        // Nodes without any edges.
        let connected: HashSet<&Node> = self.edges.iter().flat_map(|(a, b)| [*a, *b]).collect();
        for node in self.graph.nodes() {
            if !connected.contains(node) {
                members.push(Expr::Node(node.clone()));
            }
        }

        // Edges that go both ways are covered by cliques...
        while let Some((a, b)) = self.next_uncovered(|this, a, b| this.has(b, a)) {
            members.push(self.clique(a, b));
        }
        // ...and the edges that go one way by transitive tournaments.
        while let Some((a, b)) = self.next_uncovered(|_, _, _| true) {
            members.push(self.chain(a, b));
        }

        members.sort_by_cached_key(node_seq);
        match members.len() {
            0 => Expr::Disconnected(vec![]),
            1 => members.remove(0),
            _ => Expr::Disconnected(members),
        }
    }

    // The first uncovered edge matching the predicate. Edges are visited in
    // sorted order, which is what makes the cover canonical.
    fn next_uncovered(
        &self,
        pred: impl Fn(&Self, &Node, &Node) -> bool,
    ) -> Option<(&'g Node, &'g Node)> {
        self.uncovered
            .iter()
            .find(|(a, b)| pred(self, a, b))
            .copied()
    }

    // The maximal clique around the edge between a and b: grown with every
    // node adjacent both ways to all of it.
    fn clique(&mut self, a: &'g Node, b: &'g Node) -> Expr {
        let mut clique = vec![a, b];
        for c in self.graph.nodes() {
            if !clique.contains(&c) && clique.iter().all(|x| self.has(c, x) && self.has(x, c)) {
                clique.push(c);
            }
        }
        clique.sort();

        for x in &clique {
            for y in &clique {
                self.uncovered.remove(&(x, y));
            }
        }
        Expr::Connected(clique.into_iter().map(|n| Expr::Node(n.clone())).collect())
    }

    // The transitive tournament around the edge from a to b: grown with every
    // node that has an edge with all of it, inserted after the nodes pointing
    // to it, as long as it still has an uncovered edge.
    fn chain(&mut self, a: &'g Node, b: &'g Node) -> Expr {
        let mut chain = vec![a, b];
        for c in self.graph.nodes() {
            if chain.contains(&c) {
                continue;
            }
            let position = chain.iter().take_while(|x| self.has(x, c)).count();
            let (before, after) = chain.split_at(position);
            let fits = after.iter().all(|x| self.has(c, x));
            let useful = before.iter().any(|x| self.uncovered.contains(&(x, c)))
                || after.iter().any(|x| self.uncovered.contains(&(c, x)));
            if fits && useful {
                chain.insert(position, c);
            }
        }

        for (i, x) in chain.iter().enumerate() {
            for y in &chain[i + 1..] {
                self.uncovered.remove(&(x, y));
            }
        }
        Expr::Directed(chain.into_iter().map(|n| Expr::Node(n.clone())).collect())
    }
}

// The nodes of a flat expression, in order. Used to sort the members of the
// normal form.
fn node_seq(expr: &Expr) -> Vec<Node> {
    match expr {
        Expr::Node(node) => vec![node.clone()],
        _ => expr
            .members()
            .iter()
            .filter_map(|e| match e {
                Expr::Node(node) => Some(node.clone()),
                _ => None,
            })
            .collect(),
    }
}

impl Normalize for Stmt {
    fn normalize(&self) -> Self {
        match self {
            Stmt::Assign(node, expr) => Stmt::Assign(node.clone(), expr.normalize()),
        }
    }
}

impl Normalize for Program {
    fn normalize(&self) -> Self {
        Program::new(
            self.stmts.iter().map(Normalize::normalize).collect(),
            self.expr.normalize(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Normalize;
    use crate::{Expr, Parse, Program, Stmt};
    use chumsky::Parser;
    use pretty_assertions::assert_eq;

    // Asserts that every expression on the left normalizes to the one on the
    // right.
    macro_rules! assert_normalizes {
        ($($src:literal),+ => $norm:literal) => {
            $(assert_eq!(
                Expr::parse($src).unwrap().normalize(),
                Expr::parse($norm).unwrap(),
                "normalizing {}", $src,
            );)+
        };
    }

    #[test]
    fn normalize_node() {
        assert_normalizes!("A", "{A}", "{{A}}", "[A]", "[[A]]", "{[A]}", "(A)", "((A))" => "A");
    }

    #[test]
    fn normalize_empty() {
        assert_normalizes!("[]", "{}", "()", "[[], {}]", "{[], ()}" => "[]");
    }

    #[test]
    fn normalize_identity() {
        assert_normalizes!("{A, []}", "[A, []]", "(A, [])", "{[], A, ()}" => "A");
        assert_normalizes!("(A, [], B)" => "(A, B)");
        assert_normalizes!("{[], {A, B}}" => "{A, B}");
    }

    #[test]
    fn normalize_flatten() {
        assert_normalizes!("{{A, B}, C}", "{A, {B, C}}", "{A, B, C}" => "{A, B, C}");
        assert_normalizes!("[[A, B], C]", "[A, [B, C]]", "[A, B, C]" => "[A, B, C]");
        assert_normalizes!("((A, B), C)", "(A, (B, C))", "(A, B, C)" => "(A, B, C)");
        assert_normalizes!("[A, [{B, C}], D]" => "[A, {B, C}, D]");
    }

    #[test]
    fn normalize_canonical() {
        // Nodes and members come out sorted, whatever order they went in.
        assert_normalizes!("{B, A}", "{A, B}" => "{A, B}");
        assert_normalizes!("[B, A]", "[A, B]" => "[A, B]");
        assert_normalizes!("[{C, D}, {B, A}]", "[{A, B}, {C, D}]" => "[{A, B}, {C, D}]");
        assert_normalizes!("[(B, A), C]", "[C, (B, A)]" => "[(B, A), C]");
        assert_normalizes!("{S, [A, B]}" => "[{A, S}, {B, S}]");
    }

    #[test]
    fn normalize_distribute() {
        assert_normalizes!("{A, [B, C]}" => "[{A, B}, {A, C}]");
        assert_normalizes!("{{A, B}, [C, D]}", "{A, {B, [C, D]}}" => "[{A, B, C}, {A, B, D}]");
        assert_normalizes!("{[A, B], [C, D]}" => "[{A, C}, {A, D}, {B, C}, {B, D}]");
        assert_normalizes!("{A, [B, C], D}" => "[{A, B, D}, {A, C, D}]");
        assert_normalizes!("{A, [{B, C}, D], E}" => "[{A, B, C, E}, {A, D, E}]");
        assert_normalizes!(
            "{A, [{B, C}, D], E, [F, G]}" =>
            "[{A, B, C, E, F}, {A, B, C, E, G}, {A, D, E, F}, {A, D, E, G}]"
        );
        assert_normalizes!("(A, [B, C], D)" => "[(A, B, D), (A, C, D)]");
    }

    #[test]
    fn normalize_absorb() {
        assert_normalizes!("{A, A}", "(A, A)", "[A, A]" => "A");
        assert_normalizes!("{A, A, B}", "[A, {A, B}]", "[A, B, {A, B}]", "[A, {A, B}, [A]]" => "{A, B}");
        assert_normalizes!("{{A, B}, {A, B}}", "[{A, B}, {A, B}]" => "{A, B}");
        assert_normalizes!("{{A, B}, {A, B, C}, A, {C, D}, {C, D, E}}" => "{A, B, C, D, E}");
        assert_normalizes!("[{A, B}, {A, B, C}, A, {C, D}, {C, D, E}]" => "[{A, B, C}, {C, D, E}]");
        assert_normalizes!("[(A, B), (A, B, C)]", "[(A, C), (A, B, C)]" => "(A, B, C)");
        // The reverse isn't a subgraph.
        assert_normalizes!("[(B, A), (A, B, C)]" => "[{A, B}, (A, B, C)]");
    }

    #[test]
    fn normalize_disconnected_dups() {
        assert_normalizes!("[N, [I, {N, [J]}]]", "[N, I, {N, J}]" => "[I, {J, N}]");
    }

    #[test]
    fn normalize_merge() {
        assert_normalizes!("[{A, B}, {B, C}, {A, C}]" => "{A, B, C}");
        assert_normalizes!("[(A, B), (B, C), (A, C)]" => "(A, B, C)");
        assert_normalizes!("[(A, B, C), (B, A), (C, A), (C, B)]" => "{A, B, C}");
        // Two triangles sharing an edge are two cliques.
        assert_normalizes!("[{A, B}, {B, C}, {C, D}, {A, C}, {B, D}]" => "[{A, B, C}, {B, C, D}]");
        // Paths and cycles stay as they are.
        assert_normalizes!("[(A, B), (B, C)]", "A -> B -> C" => "[(A, B), (B, C)]");
        assert_normalizes!("[{A, B}, {B, C}]" => "[{A, B}, {B, C}]");
        assert_normalizes!("[(A, B), (B, C), (C, A)]" => "[(A, B), (B, C), (C, A)]");
    }

    #[test]
    fn normalize_mixed() {
        // {X, Y} = [(X, Y), (Y, X)]
        assert_normalizes!("[(A, B), (B, A)]", "{(A, B), (B, A)}", "[(A, B), {A, B}]" => "{A, B}");
        assert_normalizes!("{A, (B, C)}" => "[{A, B}, {A, C}, (B, C)]");
        assert_normalizes!("(A, {B, C}, D)" => "[(A, B, C, D), {B, C}]");
        assert_normalizes!("{A, B, (C, D)}" => "[{A, B, C}, {A, B, D}, (C, D)]");
        assert_normalizes!("{[A, B], (C, D)}" => "[{A, C}, {A, D}, {B, C}, {B, D}, (C, D)]");
    }

    #[test]
    fn normalize_repeats() {
        // A repeat carries the edge back: (X, A, Y, A, Z) = (X, {A, Y}, Z)
        assert_normalizes!("(A, B, A)", "(A, B, B, A)" => "{A, B}");
        assert_normalizes!("(A, A, B)" => "(A, B)");
        assert_normalizes!("(A, B, C, A)" => "[{A, B}, {A, C}, (B, C)]");
        assert_normalizes!("(A, B, A, C)" => "[{A, B}, (A, B, C)]");
    }

    #[test]
    fn normalize_idempotent() {
        let expr = Expr::parse("{Q, {N, [Y, (J, N)]}}").unwrap();
        assert_eq!(
            expr.normalize(),
            Expr::parse("[{J, N, Q}, {N, Q, Y}]").unwrap(),
        );
        assert_eq!(expr.normalize().normalize(), expr.normalize());
    }

    #[test]
    fn normalize_stmts() {
        assert_eq!(
            Vec::<Stmt>::parser()
                .parse(
                    r#"
                    G1 = {A, [B, C]}
                    G2 = [[{{D}}]]
                    "#
                )
                .unwrap()
                .iter()
                .map(Normalize::normalize)
                .collect::<Vec<_>>(),
            Vec::<Stmt>::parser()
                .parse(
                    r#"
                    G1 = [{A, B}, {A, C}]
                    G2 = D
                "#
                )
                .unwrap(),
        );
    }

    #[test]
    fn normalize_program() {
        assert_eq!(
            Program::parser()
                .parse(
                    r#"
                    G1 = {A, [B, C]}
                    [[{{D}}]]
                    "#
                )
                .unwrap()
                .normalize(),
            Program::parser()
                .parse(
                    r#"
                    G1 = [{A, B}, {A, C}]
                    D
                "#
                )
                .unwrap(),
        );
    }
}
