//! Reductions to normal form.
//!
//! Normalizing rewrites an expression into a smaller one describing the same
//! [`Graph`]:
//!
//! - Empty graphs are fully disconnected: `{} => []`, `() => []`
//! - Single node graphs are just the node: `[{A}] => A`
//! - Fully connected graphs are flattened: `{{A, B}, C} => {A, B, C}`
//! - Directed graphs are flattened the same way, keeping their order:
//!   `((A, B), C) => (A, B, C)`
//! - Connected and directed expressions distribute over disconnected ones:
//! ```grapl
//! {[A, B], [C, D]} =>
//! [{A, C}, {A, D}, {B, C}, {B, D}]
//! ```
//! - Connected expressions containing anything directed are lowered to the
//!   pair of directed expressions they abbreviate, since
//!   `{X, Y} = [(X, Y), (Y, X)]`:
//! ```grapl
//! {A, (B, C)} =>
//! [(A, B, C), (B, C, A)]
//! ```
//! - A node repeated in a directed expression is connected both ways to
//!   everything between the repeats: `(A, B, A) => {A, B}`
//! - Siblings whose nodes form a clique or a transitive tournament in the whole
//!   graph are merged: `[{A, B}, {B, C}, {A, C}] => {A, B, C}` and
//!   `[(A, B), (B, C), (A, C)] => (A, B, C)`
//! - Anything that is a subgraph of a sibling is dropped: `[A, {A, B}] => {A, B}`
//!
//! The normal form is a disconnected list of cliques `{...}` and transitive
//! tournaments `(...)`, none of which is a subgraph of another. It is not
//! guaranteed to be canonical: `[(A, B, C), (B, A)]` and `[(A, B, C), (B, A, C)]`
//! are the same graph but both are already normal. Compare [`Expr::graph`]s to
//! compare graphs.

use crate::{Expr, Graph, Node, Program, Stmt};
use std::collections::{HashMap, HashSet};

pub trait Normalize: Sized {
    fn normalize(&self) -> Self;
}

// The node sequences a normalized expression contributes as a member of a
// connected or directed expression. A disconnected member contributes one
// alternative per element, which is what distributes `{A, [B, C]}` into
// `[{A, B}, {A, C}]`. An empty disconnected member contributes nothing, so it
// is the identity: `{A, []} => A`.
//
// Inside a directed expression a connected member stands for both of its
// orderings, since `{B, C} = [(B, C), (C, B)]`.
fn alternatives(expr: &Expr, directed: bool) -> Vec<Vec<Expr>> {
    match expr {
        e @ Expr::Node(_) => vec![vec![e.clone()]],
        Expr::Connected(cs) if directed && cs.len() > 1 => {
            vec![cs.clone(), cs.iter().rev().cloned().collect()]
        }
        Expr::Connected(cs) | Expr::Directed(cs) => vec![cs.clone()],
        Expr::Disconnected(ds) => ds.iter().flat_map(|d| alternatives(d, directed)).collect(),
    }
}

// Rewrites a directed sequence with a repeated node. Everything between the
// first and last occurrence is connected to it in both directions:
// (X, A, Y, A, Z) => (X, {A, Y}, Z)
//
// Adjacent repeats are the special case (A, A) => {A, []} => A. The result is
// shorter once normalized, so repeating this terminates.
fn unrepeat(seq: &[Expr]) -> Option<Expr> {
    let (i, j) = seq.iter().enumerate().find_map(|(i, node)| {
        let j = seq.iter().rposition(|other| other == node)?;
        (j > i).then_some((i, j))
    })?;
    let mut fresh = seq[..i].to_vec();
    fresh.push(Expr::Connected(vec![
        seq[i].clone(),
        Expr::Directed(seq[i + 1..j].to_vec()),
    ]));
    fresh.extend_from_slice(&seq[j + 1..]);
    Some(Expr::Directed(fresh))
}

// The nodes of a normalized connected or directed expression, in order.
fn node_seq(expr: &Expr) -> Vec<Node> {
    match expr {
        Expr::Node(node) => vec![node.clone()],
        _ => expr.members().iter().flat_map(node_seq).collect(),
    }
}

// The out-neighbours of every node, for quick edge lookups.
type Adjacency<'g> = HashMap<&'g Node, HashSet<&'g Node>>;

fn adjacency(graph: &Graph) -> Adjacency<'_> {
    let mut adj = Adjacency::new();
    for (a, b) in graph.edges() {
        adj.entry(a).or_default().insert(b);
    }
    adj
}

// A member of a disconnected expression being merged, with its graph and node
// sequence computed once up front.
struct Member {
    expr: Expr,
    graph: Graph,
    nodes: Vec<Node>,
}

impl Member {
    fn new(expr: Expr) -> Self {
        let graph = expr.graph();
        let nodes = node_seq(&expr);
        Member { expr, graph, nodes }
    }
}

// The clique or transitive tournament on the nodes of both members, if the
// whole graph has every edge that needs. A result that is just one of the two
// members is left to `dedup`, which keeps the survivor in place.
fn merged(adj: &Adjacency, a: &Member, b: &Member) -> Option<Member> {
    let edge = |x: &Node, y: &Node| adj.get(x).is_some_and(|out| out.contains(y));

    // Most pairs can't merge. Check one pair of nodes they don't share before
    // doing anything that allocates.
    let x = a.nodes.iter().find(|x| !b.nodes.contains(x));
    let y = b.nodes.iter().find(|y| !a.nodes.contains(y));
    if let (Some(x), Some(y)) = (x, y) {
        if !edge(x, y) && !edge(y, x) {
            return None;
        }
    }

    let mut nodes = a.nodes.clone();
    for node in &b.nodes {
        if !nodes.contains(node) {
            nodes.push(node.clone());
        }
    }
    let n = nodes.len();

    let count = nodes
        .iter()
        .map(|x| nodes.iter().filter(|y| *x != **y && edge(x, y)).count())
        .sum::<usize>();

    let result = if count == n * (n - 1) {
        // {A, B, C}: every pair both ways. Neither member's order is
        // meaningful for the merged clique, so sort it.
        nodes.sort();
        Expr::Connected(nodes.into_iter().map(Expr::Node).collect())
    } else if count == n * (n - 1) / 2 {
        // (A, B, C): every pair one way, all agreeing with the order by
        // out-degree.
        let mut by_degree: Vec<(usize, Node)> = nodes
            .iter()
            .map(|x| (nodes.iter().filter(|y| edge(x, y)).count(), x.clone()))
            .collect();
        by_degree.sort_by_key(|(degree, _)| std::cmp::Reverse(*degree));
        let nodes: Vec<Node> = by_degree.into_iter().map(|(_, x)| x).collect();
        let ordered = nodes
            .iter()
            .enumerate()
            .all(|(i, x)| nodes[i + 1..].iter().all(|y| edge(x, y)));
        if !ordered {
            return None;
        }
        Expr::Directed(nodes.into_iter().map(Expr::Node).collect())
    } else {
        return None;
    };

    let result = Member::new(result);
    if result.graph == a.graph || result.graph == b.graph {
        return None;
    }
    Some(result)
}

impl Expr {
    fn flatten(&self) -> Self {
        match self {
            Expr::Node(node) => Expr::Node(node.clone()),
            Expr::Connected(exprs) => {
                let exprs: Vec<Expr> = exprs.iter().map(Expr::normalize).collect();
                if exprs.iter().any(Expr::is_directed) {
                    // {X, Y} = [(X, Y), (Y, X)]
                    let rev = exprs.iter().rev().cloned().collect();
                    Expr::Disconnected(vec![Expr::Directed(exprs), Expr::Directed(rev)]).flatten()
                } else {
                    Self::distribute(exprs, false)
                }
            }
            Expr::Directed(exprs) => {
                Self::distribute(exprs.iter().map(Expr::normalize).collect(), true)
            }
            Expr::Disconnected(exprs) => {
                // [A, [B, C], {D, E}] => [A, B, C, {D, E}]
                let mut ds: Vec<Expr> = exprs
                    .iter()
                    .map(Expr::normalize)
                    .flat_map(|e| match e {
                        Expr::Disconnected(dexprs) => dexprs,
                        e => vec![e],
                    })
                    .collect();

                if ds.len() == 1 {
                    // [A] => A
                    ds.remove(0)
                } else {
                    Expr::Disconnected(ds)
                }
            }
        }
    }

    // Distributes a connected or directed expression over its disconnected
    // members. The general reduction strategy follows these steps:
    // {A, [B, C], D, [E, F]} =>
    // [{A}, [B, C], D, [E, F]] =>
    // [{A, B}, {A, C}, D, [E, F]] =>
    // [{A, B, D}, {A, C, D}, [E, F]] =>
    // [{A, B, D, E}, {A, B, D, F}, {A, C, D, E}, {A, C, D, F}]
    //
    // Expects normalized members.
    fn distribute(exprs: Vec<Expr>, directed: bool) -> Expr {
        let variant = if directed {
            Expr::Directed
        } else {
            Expr::Connected
        };

        let mut seqs: Vec<Vec<Expr>> = vec![vec![]];
        for expr in &exprs {
            let alts = alternatives(expr, directed);
            if alts.is_empty() {
                continue;
            }
            seqs = seqs
                .iter()
                .flat_map(|seq| {
                    alts.iter().map(move |alt| {
                        let mut fresh = seq.clone();
                        fresh.extend(alt.iter().cloned());
                        fresh
                    })
                })
                .collect();
        }

        let mut members: Vec<Expr> = vec![];
        for mut seq in seqs {
            if directed {
                // (A, B, A) => {A, B}
                if let Some(rewritten) = unrepeat(&seq) {
                    match rewritten.normalize() {
                        Expr::Disconnected(ds) => members.extend(ds),
                        e => members.push(e),
                    }
                    continue;
                }
            } else {
                // A node repeated in a clique adds nothing: {A, A, B} => {A, B}
                let mut seen = HashSet::new();
                seq.retain(|node| seen.insert(node.clone()));
            }
            members.push(match seq.len() {
                0 => Expr::Disconnected(vec![]),
                // {A} => A
                1 => seq.remove(0),
                _ => variant(seq),
            });
        }

        if members.len() == 1 {
            members.remove(0)
        } else {
            Expr::Disconnected(members)
        }
    }

    // Drops members that are subgraphs of other members. This only works on
    // normalized expressions.
    fn dedup(&self) -> Self {
        match self {
            e @ Expr::Node(_) => e.clone(),
            // Members of a directed expression are ordered nodes, and a repeat
            // like (A, B, A) carries the edge B->A, so there's nothing to drop.
            e @ Expr::Directed(_) => e.clone(),
            Expr::Connected(exprs) => Expr::Connected(Self::dedup_exprs(exprs)),
            Expr::Disconnected(exprs) => Expr::Disconnected(Self::dedup_exprs(exprs)),
        }
    }

    fn dedup_exprs(exprs: &[Expr]) -> Vec<Expr> {
        let mut fresh: Vec<(Expr, Graph)> = Vec::new();
        for expr in exprs {
            let graph = expr.graph();
            // Skip anything already covered by an existing expr.
            if fresh.iter().any(|(_, f)| graph.is_subgraph(f)) {
                continue;
            }
            // Replace every existing expr this one covers.
            fresh.retain(|(_, f)| !f.is_subgraph(&graph));
            fresh.push((expr.clone(), graph));
        }
        fresh.into_iter().map(|(e, _)| e).collect()
    }

    // Merges siblings whose nodes together form a clique or a transitive
    // tournament in the whole graph. A directed expression and its reverse is
    // the simplest case: [(A, B), (B, A)] => {A, B}
    //
    // This must run before `dedup`, since two different orderings of the same
    // clique only become duplicates once they've been merged.
    fn merge(&self) -> Self {
        let Expr::Disconnected(exprs) = self else {
            return self.clone();
        };
        let graph = self.graph();
        let adj = adjacency(&graph);
        let mut members: Vec<Member> = exprs.iter().cloned().map(Member::new).collect();
        'merging: loop {
            for i in 0..members.len() {
                for j in i + 1..members.len() {
                    if let Some(merged) = merged(&adj, &members[i], &members[j]) {
                        members[i] = merged;
                        members.remove(j);
                        continue 'merging;
                    }
                }
            }
            break;
        }
        Expr::Disconnected(members.into_iter().map(|m| m.expr).collect())
    }
}

impl Normalize for Expr {
    fn normalize(&self) -> Self {
        self.flatten().merge().dedup().flatten()
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
        let norm_stmts = self.stmts.iter().map(Normalize::normalize).collect();
        let norm_expr = self.expr.normalize();
        Program::new(norm_stmts, norm_expr)
    }
}

#[cfg(test)]
mod tests {
    use super::Normalize;
    use crate::{Expr, Parse, Program, Stmt};
    use chumsky::Parser;
    use pretty_assertions::assert_eq;

    #[test]
    fn normalize_node() {
        assert_eq!(
            Expr::parse("A").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("A").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
    }

    #[test]
    fn normalize_empty_expr() {
        assert_eq!(
            Expr::parse("[]").unwrap().normalize(),
            Expr::parse("[]").unwrap(),
        );
        assert_eq!(
            Expr::parse("{}").unwrap().normalize(),
            Expr::parse("[]").unwrap(),
        );
    }

    #[test]
    fn dedup_expr() {
        assert_eq!(Expr::parse("A").unwrap().dedup(), Expr::parse("A").unwrap(),);
        assert_eq!(
            Expr::parse("[]").unwrap().dedup(),
            Expr::parse("[]").unwrap(),
        );
        assert_eq!(
            Expr::parse("{}").unwrap().dedup(),
            Expr::parse("{}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{A,A}").unwrap().dedup(),
            Expr::parse("{A}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{A,A,B}").unwrap().dedup(),
            Expr::parse("{A,B}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A},{A}}").unwrap().dedup(),
            Expr::parse("{{A}}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A,B},{A,B}}").unwrap().dedup(),
            Expr::parse("{{A,B}}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A,B},{A,B,C}}").unwrap().dedup(),
            Expr::parse("{{A,B,C}}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A,B},{A,B,C},A,{C,D},{C,D,E}}")
                .unwrap()
                .dedup(),
            Expr::parse("{{A,B,C},{C,D,E}}").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A,{A,B}]").unwrap().dedup(),
            Expr::parse("[{A,B}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[{A,B},{A,B,C},A,{C,D},{C,D,E}]")
                .unwrap()
                .dedup(),
            Expr::parse("[{A,B,C},{C,D,E}]").unwrap(),
        );
    }

    #[test]
    fn normalize_nested_expr() {
        assert_eq!(
            Expr::parse("{A}").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A}}").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A]").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("[[A]]").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("{[A]}").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parser()
                .parse("[A, [{B, C}], D]")
                .unwrap()
                .normalize(),
            Expr::parse("[A, {B, C}, D]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A, [B, C], D]").unwrap().normalize(),
            Expr::parse("[A, B, C, D]").unwrap(),
        );
    }

    #[test]
    fn normalize_expr() {
        assert_eq!(
            Expr::parse("{A, [B]}").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A, {B}]").unwrap().normalize(),
            Expr::parse("[A, B]").unwrap(),
        );

        assert_eq!(
            Expr::parse("{A, [B, C]}").unwrap().normalize(),
            Expr::parse("[{A, B}, {A, C}]").unwrap(),
        );
        assert_eq!(
            Expr::parser()
                .parse("{{A, B}, [C, D]}")
                .unwrap()
                .normalize(),
            Expr::parse("[{A, B, C}, {A, B, D}]").unwrap(),
        );
        assert_eq!(
            Expr::parser()
                .parse("{[A, B], [C, D]}")
                .unwrap()
                .normalize(),
            Expr::parser()
                .parse("[{A, C}, {A, D}, {B, C}, {B, D}]")
                .unwrap(),
        );
        assert_eq!(
            Expr::parser()
                .parse("{A, {B, [C, D]}}")
                .unwrap()
                .normalize(),
            Expr::parse("[{A, B, C}, {A, B, D}]").unwrap(),
        );

        assert_eq!(
            Expr::parse("{A, [B, C], D}").unwrap().normalize(),
            Expr::parse("[{A, B, D}, {A, C, D}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A, {B, C}, D]").unwrap().normalize(),
            Expr::parse("[A, {B, C}, D]").unwrap(),
        );

        assert_eq!(
            Expr::parser()
                .parse("{A, [{B, C}, D], E}")
                .unwrap()
                .normalize(),
            Expr::parse("[{A, B, C, E}, {A, D, E}]").unwrap(),
        );

        assert_eq!(
            Expr::parser()
                .parse("{A, [{B, C}, D], E, [F, G]}")
                .unwrap()
                .normalize(),
            Expr::parser()
                .parse(
                    "[
                    {A, B, C, E, F},
                    {A, B, C, E, G},
                    {A, D, E, F},
                    {A, D, E, G},
                    ]"
                )
                .unwrap(),
        );

        assert_eq!(
            Expr::parse("[A,{A,B},[A]]").unwrap().normalize(),
            Expr::parse("{A,B}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{{A,B},{A,B,C},A,{C,D},{C,D,E}}")
                .unwrap()
                .normalize(),
            Expr::parse("{A,B,C,D,E}").unwrap(),
        );
    }

    #[test]
    fn disconnected_dups() {
        assert_eq!(
            Expr::parse("[N,[I,{N,[J]}]]").unwrap().normalize(),
            Expr::parse("[I,{N,J}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[N,I,{N,J}]").unwrap().dedup(),
            Expr::parse("[I,{N,J}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[A,B,{A,B}]").unwrap().dedup(),
            Expr::parse("[{A,B}]").unwrap(),
        );
    }

    #[test]
    fn normalize_empty_identity() {
        assert_eq!(
            Expr::parse("()").unwrap().normalize(),
            Expr::parse("[]").unwrap(),
        );
        assert_eq!(
            Expr::parse("{A, []}").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, [], B)").unwrap().normalize(),
            Expr::parse("(A, B)").unwrap(),
        );
        assert_eq!(
            Expr::parse("{[], {A, B}}").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
    }

    #[test]
    fn normalize_directed_expr() {
        assert_eq!(
            Expr::parse("(A)").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, A, B)").unwrap().normalize(),
            Expr::parse("(A, B)").unwrap(),
        );
        assert_eq!(
            Expr::parse("((A, B), C)").unwrap().normalize(),
            Expr::parse("(A, B, C)").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, [B, C], D)").unwrap().normalize(),
            Expr::parse("[(A, B, D), (A, C, D)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, {B, C}, D)").unwrap().normalize(),
            Expr::parse("[(A, B, C, D), (A, C, B, D)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("A -> B -> C").unwrap().normalize(),
            Expr::parse("[(A, B), (B, C)]").unwrap(),
        );
    }

    #[test]
    fn normalize_mixed_expr() {
        // {X, Y} = [(X, Y), (Y, X)]
        assert_eq!(
            Expr::parse("{A, (B, C)}").unwrap().normalize(),
            Expr::parse("[(A, B, C), (B, C, A)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("{[A, B], (C, D)}").unwrap().normalize(),
            Expr::parse("[(A, C, D), (B, C, D), (C, D, A), (C, D, B)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("{A, B, (C, D)}").unwrap().normalize(),
            Expr::parse("[(A, B, C, D), (C, D, B, A)]").unwrap(),
        );
        // ... and back again.
        assert_eq!(
            Expr::parse("[(A, B), (B, A)]").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
        assert_eq!(
            Expr::parse("[(A, B), {A, B}]").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
    }

    #[test]
    fn normalize_idempotent() {
        // Lowering {Q, [{N, Y}, ...]} produces both orderings of {Q, N, Y},
        // which merge into the same clique and must then be deduplicated.
        let expr = Expr::parse("{Q, {N, [Y, (J, N)]}}").unwrap();
        assert_eq!(
            expr.normalize(),
            Expr::parse("[{Q, N, Y}, {Q, J, N}]").unwrap(),
        );
        assert_eq!(expr.normalize().normalize(), expr.normalize());
    }

    #[test]
    fn directed_dups() {
        // A subsequence is a subgraph.
        assert_eq!(
            Expr::parse("[(A, B), (A, B, C)]").unwrap().normalize(),
            Expr::parse("(A, B, C)").unwrap(),
        );
        assert_eq!(
            Expr::parse("[(A, C), (A, B, C)]").unwrap().normalize(),
            Expr::parse("(A, B, C)").unwrap(),
        );
        // The reverse isn't.
        assert_eq!(
            Expr::parse("[(B, A), (A, B, C)]").unwrap().normalize(),
            Expr::parse("[(B, A), (A, B, C)]").unwrap(),
        );
    }

    #[test]
    fn normalize_repeats() {
        // A repeat carries the edge back.
        assert_eq!(
            Expr::parse("(A, B, A)").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
        assert_eq!(
            Expr::parse("{(A, B), (B, A)}").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
        // (A, B, C, A) => {A, (B, C)}
        assert_eq!(
            Expr::parse("(A, B, C, A)").unwrap().normalize(),
            Expr::parse("[(A, B, C), (B, C, A)]").unwrap(),
        );
        // (A, B, A, C) => ({A, B}, C)
        assert_eq!(
            Expr::parse("(A, B, A, C)").unwrap().normalize(),
            Expr::parse("[(A, B, C), (B, A, C)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, A)").unwrap().normalize(),
            Expr::parse("A").unwrap(),
        );
        assert_eq!(
            Expr::parse("(A, B, B, A)").unwrap().normalize(),
            Expr::parse("{A, B}").unwrap(),
        );
    }

    #[test]
    fn normalize_merge() {
        assert_eq!(
            Expr::parse("[{A, B}, {B, C}, {A, C}]").unwrap().normalize(),
            Expr::parse("{A, B, C}").unwrap(),
        );
        assert_eq!(
            Expr::parse("[(A, B), (B, C), (A, C)]").unwrap().normalize(),
            Expr::parse("(A, B, C)").unwrap(),
        );
        assert_eq!(
            Expr::parse("[(A, B, C), (B, A), (C, A), (C, B)]")
                .unwrap()
                .normalize(),
            Expr::parse("{A, B, C}").unwrap(),
        );
        // Two triangles sharing an edge are two cliques.
        assert_eq!(
            Expr::parse("[{A, B}, {B, C}, {C, D}, {A, C}, {B, D}]")
                .unwrap()
                .normalize(),
            Expr::parse("[{A, B, C}, {B, C, D}]").unwrap(),
        );
        // Paths and cycles stay as they are.
        assert_eq!(
            Expr::parse("[(A, B), (B, C)]").unwrap().normalize(),
            Expr::parse("[(A, B), (B, C)]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[{A, B}, {B, C}]").unwrap().normalize(),
            Expr::parse("[{A, B}, {B, C}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[(A, B), (B, C), (C, A)]").unwrap().normalize(),
            Expr::parse("[(A, B), (B, C), (C, A)]").unwrap(),
        );
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
    fn normalize_ret() {
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
