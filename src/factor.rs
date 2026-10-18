//! Factoring.
//!
//! The normal form is a sum of products, and that can be exponentially longer
//! than the expression it came from: `{[A, B], [C, D], [E, F]}` normalizes to
//! seven cliques. Factoring goes the other way, nesting the three bracket
//! forms as deeply as the graph allows.
//!
//! This is modular decomposition. A module is a set of nodes that every node
//! outside it relates to in the same way, and any graph is one of
//!
//! - `[...]` of its connected components,
//! - `{...}` of parts with every edge between them going both ways,
//! - `(...)` of parts with every edge between them going one way, forwards,
//! - or prime: its maximal modules are found, the quotient graph on them is
//!   covered as in [`Normalize`](crate::Normalize), and each module is
//!   factored in turn.
//!
//! The three bracket forms are exactly the three ways a graph can decompose
//! without a prime part. A graph with none (a cograph, when undirected)
//! factors into an expression mentioning every node once, which is as small
//! as it gets. Like the normal form, the result depends only on the graph.

use crate::{Expr, Graph, Node};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};

impl Expr {
    /// The most deeply nested expression for this graph, see [`factor`](self).
    pub fn factor(&self) -> Expr {
        self.graph().factor()
    }
}

impl Graph {
    /// The most deeply nested expression for this graph, see [`factor`](self).
    pub fn factor(&self) -> Expr {
        Factoring::new(self).factor()
    }
}

struct Factoring<'g> {
    graph: &'g Graph,
    nodes: Vec<&'g Node>,
    edges: HashSet<(&'g Node, &'g Node)>,
}

impl<'g> Factoring<'g> {
    fn new(graph: &'g Graph) -> Self {
        Factoring {
            graph,
            nodes: graph.nodes().iter().collect(),
            edges: graph.edges().iter().map(|(a, b)| (a, b)).collect(),
        }
    }

    fn has(&self, a: &Node, b: &Node) -> bool {
        self.edges.contains(&(a, b))
    }

    fn both(&self, a: &Node, b: &Node) -> bool {
        self.has(a, b) && self.has(b, a)
    }

    fn one_way(&self, a: &Node, b: &Node) -> bool {
        self.has(a, b) && !self.has(b, a)
    }

    fn factor(&self) -> Expr {
        match self.nodes.len() {
            0 => return Expr::Disconnected(vec![]),
            1 => return Expr::Node(self.nodes[0].clone()),
            _ => {}
        }

        // [A, B]: nothing between the parts.
        let parts = self.components(|a, b| self.has(a, b) || self.has(b, a));
        if parts.len() > 1 {
            return sorted(Expr::Disconnected, self.factor_parts(&parts));
        }

        // {A, B}: everything between the parts, both ways.
        let parts = self.components(|a, b| !self.both(a, b));
        if parts.len() > 1 {
            return sorted(Expr::Connected, self.factor_parts(&parts));
        }

        // (A, B): everything between the parts, one way, forwards.
        if let Some(parts) = self.order() {
            return Expr::Directed(self.factor_parts(&parts));
        }

        // Prime: cover the quotient on the maximal modules, then factor each
        // module in place of its representative.
        let modules = self.modules();
        if modules.len() == self.nodes.len() || modules.len() == 1 {
            return Expr::from(self.graph);
        }
        let representative = |module: &BTreeSet<Node>| module.first().unwrap().clone();
        let mut quotient = Graph::empty();
        for a in &modules {
            quotient = quotient.overlay(Graph::node(representative(a)));
            for b in &modules {
                if a != b && self.has(a.first().unwrap(), b.first().unwrap()) {
                    let edge =
                        Graph::node(representative(a)).connect(Graph::node(representative(b)));
                    quotient = quotient.overlay(edge);
                }
            }
        }
        let factored: BTreeMap<Node, Expr> = modules
            .iter()
            .map(|module| (representative(module), self.graph.induced(module).factor()))
            .collect();
        substitute(&Expr::from(&quotient), &factored)
    }

    fn factor_parts(&self, parts: &[BTreeSet<Node>]) -> Vec<Expr> {
        parts
            .iter()
            .map(|part| self.graph.induced(part).factor())
            .collect()
    }

    // The connected components under the given relation.
    fn components(&self, related: impl Fn(&Node, &Node) -> bool) -> Vec<BTreeSet<Node>> {
        let mut seen: HashSet<&Node> = HashSet::new();
        let mut parts = vec![];
        for &start in &self.nodes {
            if seen.contains(start) {
                continue;
            }
            let mut part = BTreeSet::new();
            let mut stack = vec![start];
            while let Some(x) = stack.pop() {
                if !seen.insert(x) {
                    continue;
                }
                part.insert(x.clone());
                stack.extend(
                    self.nodes
                        .iter()
                        .filter(|&&y| !seen.contains(y) && related(x, y)),
                );
            }
            parts.push(part);
        }
        parts
    }

    // The parts of a linear order, if this graph is one: a cut between parts
    // has every edge from before it to after it going one way, forwards.
    //
    // Nodes before a cut have strictly more one-way out-edges than nodes after
    // it, so sorting by that puts the parts in order and the cuts are then
    // just the prefixes to check.
    fn order(&self) -> Option<Vec<BTreeSet<Node>>> {
        let mut nodes = self.nodes.clone();
        let out_degree = |x: &Node| self.nodes.iter().filter(|y| self.one_way(x, y)).count();
        nodes.sort_by_cached_key(|x| Reverse(out_degree(x)));

        let mut parts = vec![];
        let mut start = 0;
        for cut in 1..nodes.len() {
            let (before, after) = nodes.split_at(cut);
            if before
                .iter()
                .all(|x| after.iter().all(|y| self.one_way(x, y)))
            {
                parts.push(nodes[start..cut].iter().map(|n| (*n).clone()).collect());
                start = cut;
            }
        }
        parts.push(nodes[start..].iter().map(|n| (*n).clone()).collect());
        (parts.len() > 1).then_some(parts)
    }

    // The maximal proper modules, which partition the nodes of a graph that
    // isn't one of the three forms above. Every module is a union of the
    // smallest modules around pairs of nodes, so those are found and any that
    // overlap are merged.
    fn modules(&self) -> Vec<BTreeSet<Node>> {
        let mut modules: Vec<BTreeSet<&Node>> = vec![];
        for (i, &u) in self.nodes.iter().enumerate() {
            for &v in &self.nodes[i + 1..] {
                let mut module = self.closure(u, v);
                if module.len() == self.nodes.len() {
                    continue;
                }
                while let Some(i) = modules.iter().position(|m| !m.is_disjoint(&module)) {
                    module.extend(modules.remove(i));
                }
                modules.push(module);
            }
        }
        for &x in &self.nodes {
            if !modules.iter().any(|m| m.contains(x)) {
                modules.push(BTreeSet::from([x]));
            }
        }
        modules
            .into_iter()
            .map(|m| m.into_iter().cloned().collect())
            .collect()
    }

    // The smallest module containing both nodes: grown with every node that
    // relates differently to two of its members.
    fn closure(&self, u: &'g Node, v: &'g Node) -> BTreeSet<&'g Node> {
        let mut module = BTreeSet::from([u, v]);
        loop {
            let relation = |x: &Node, s: &Node| (self.has(x, s), self.has(s, x));
            let distinguishes = |x: &&Node| module.iter().any(|s| relation(x, s) != relation(x, u));
            let fresh: Vec<_> = self
                .nodes
                .iter()
                .filter(|x| !module.contains(*x) && distinguishes(x))
                .copied()
                .collect();
            if fresh.is_empty() {
                return module;
            }
            module.extend(fresh);
        }
    }
}

// Nodes first, then everything else, each sorted by their nodes.
fn sorted(variant: fn(Vec<Expr>) -> Expr, mut exprs: Vec<Expr>) -> Expr {
    exprs.sort_by_cached_key(|e| (!matches!(e, Expr::Node(_)), e.nodes()));
    variant(exprs)
}

// Replaces the given nodes with expressions.
fn substitute(expr: &Expr, with: &BTreeMap<Node, Expr>) -> Expr {
    let substituted =
        |exprs: &[Expr]| -> Vec<Expr> { exprs.iter().map(|e| substitute(e, with)).collect() };
    match expr {
        Expr::Node(node) => with.get(node).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Connected(exprs) => sorted(
            Expr::Connected,
            splice(substituted(exprs), |e| matches!(e, Expr::Connected(_))),
        ),
        Expr::Disconnected(exprs) => sorted(
            Expr::Disconnected,
            splice(substituted(exprs), |e| matches!(e, Expr::Disconnected(_))),
        ),
        Expr::Directed(exprs) => Expr::Directed(splice(substituted(exprs), |e| {
            matches!(e, Expr::Directed(_))
        })),
    }
}

// All three forms are associative, so members of the same form as their
// parent are spliced in: {A, {B, E}} => {A, B, E}
fn splice(exprs: Vec<Expr>, same: impl Fn(&Expr) -> bool) -> Vec<Expr> {
    exprs
        .into_iter()
        .flat_map(|e| {
            if same(&e) {
                e.members().to_vec()
            } else {
                vec![e]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{Expr, Normalize, Parse};
    use pretty_assertions::assert_eq;

    // Asserts that every expression on the left factors to the one on the
    // right.
    macro_rules! assert_factors {
        ($($src:literal),+ => $factored:literal) => {
            $(assert_eq!(
                Expr::parse($src).unwrap().factor(),
                Expr::parse($factored).unwrap(),
                "factoring {}", $src,
            );)+
        };
    }

    #[test]
    fn factor_trivial() {
        assert_factors!("[]", "{}", "()" => "[]");
        assert_factors!("A", "{A}", "[[A]]", "(A, A)" => "A");
        assert_factors!("{A, B}", "[(A, B), (B, A)]" => "{A, B}");
        assert_factors!("[A, B]", "[B, A]" => "[A, B]");
        assert_factors!("(A, B)", "A -> B" => "(A, B)");
    }

    #[test]
    fn factor_cotrees() {
        // Expressions with no prime part come back as they went in, up to
        // ordering, with nodes first.
        assert_factors!("{S, [A, B, C, D]}", "[{A, S}, {B, S}, {C, S}, {D, S}]" => "{S, [A, B, C, D]}");
        assert_factors!("[A, {B, C}, D]" => "[A, D, {B, C}]");
        assert_factors!("{A, [B, {C, D}]}", "{[{C, D}, B], A}" => "{A, [B, {C, D}]}");
        assert_factors!("{[A, B], [C, D], [E, F]}" => "{[A, B], [C, D], [E, F]}");
        // ...and so does the normal form, however long it got.
        let octahedron = Expr::parse("{[A, B], [C, D], [E, F]}").unwrap();
        assert_eq!(octahedron.normalize().members().len(), 7);
        assert_eq!(octahedron.normalize().factor(), octahedron);
        // A 4-cycle is complete bipartite.
        assert_factors!("[{A, B}, {B, C}, {C, D}, {D, A}]" => "{[A, C], [B, D]}");
    }

    #[test]
    fn factor_prime() {
        // A path on four nodes has no modules, so it stays a sum of products.
        assert_factors!("[{A, B}, {B, C}, {C, D}]" => "[{A, B}, {B, C}, {C, D}]");
        // Doubling one of its nodes gives it a module.
        assert_factors!(
            "[{A, B}, {A, E}, {B, C}, {C, E}, {C, D}]" =>
            "[{A, [B, E]}, {C, [B, E]}, {C, D}]"
        );
        assert_factors!(
            "[{A, B}, {A, E}, {B, C}, {C, E}, {C, D}, {B, E}]" =>
            "[{A, B, E}, {B, C, E}, {C, D}]"
        );
    }

    #[test]
    fn factor_directed() {
        assert_factors!("(A, {B, C}, D)", "[(A, B, C, D), {B, C}]" => "(A, {B, C}, D)");
        assert_factors!("(A, [B, C], D)", "[(A, B, D), (A, C, D)]" => "(A, [B, C], D)");
        assert_factors!("{A, (B, C)}", "[{A, B}, {A, C}, (B, C)]" => "{A, (B, C)}");
        assert_factors!("{A, B, (C, D)}" => "{A, B, (C, D)}");
        // Paths and cycles are prime.
        assert_factors!("A -> B -> C" => "[(A, B), (B, C)]");
        assert_factors!("[(A, B), (B, C), (C, A)]" => "[(A, B), (B, C), (C, A)]");
        // A directed path with a doubled middle.
        assert_factors!("[(A, B, C), (A, E, C)]" => "(A, [B, E], C)");
        assert_factors!("[(A, B), (A, E), (B, C), (E, C)]" => "[(A, [B, E]), ([B, E], C)]");
    }

    #[test]
    fn factor_idempotent() {
        for src in [
            "{Q, {N, [Y, (J, N)]}}",
            "[{A, B}, {A, E}, {B, C}, {C, E}, {C, D}]",
            "{[A, B], (C, D)}",
        ] {
            let factored = Expr::parse(src).unwrap().factor();
            assert_eq!(factored.factor(), factored, "factoring {}", src);
            assert_eq!(factored.graph(), Expr::parse(src).unwrap().graph());
        }
    }
}
