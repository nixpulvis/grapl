use std::collections::HashSet;

use crate::{Expr, Node, Ret, Stmt};

/// Reductions to normal form.
///
/// - Empty graphs are fully disconnected: `{} => []`, `() => []`
/// - Single node graphs are just the node: `[{A}] => A`
/// - Fully connected graphs are flattened: `{{A, B}, C} => {A, B, C}`
/// - Directed graphs are flattened the same way, keeping their order:
///   `((A, B), C) => (A, B, C)`
/// - Normalized graphs with disconnected expressions all collapse into a single
///   fully disconnected expression:
/// ```grapl
/// {[A, B], [C, D]} =>
/// [{A, C}, {A, D}, {B, C}, {B, D}]
/// ```
/// - Connected expressions containing anything directed are lowered to the
///   pair of directed expressions they abbreviate, since
///   `{X, Y} = [(X, Y), (Y, X)]`:
/// ```grapl
/// {A, (B, C)} =>
/// [(A, B, C), (B, C, A)]
/// ```
///
/// The normal form is a disconnected list of cliques `{...}` and transitive
/// tournaments `(...)`, none of which is a subgraph of another. It is not
/// canonical: `{A, B, C}` and `[{A, B}, {B, C}, {A, C}]` are the same graph but
/// both are already normal. Use [`Expr::edges`] to compare graphs.
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
                // Collect a list of disconnected nodes.
                let mut ds = vec![];
                for expr in exprs {
                    match expr.normalize() {
                        // ds = [A,B]
                        // expr = {C,D}
                        // ds <= [A,B,{C,D}]
                        e @ Expr::Node(_) | e @ Expr::Connected(_) | e @ Expr::Directed(_) => {
                            ds.push(e)
                        }
                        // ds = [A,B]
                        // expr = [C,D]
                        // ds <= [A,B,C,D]
                        Expr::Disconnected(dexprs) => {
                            for dexpr in dexprs {
                                ds.push(dexpr);
                            }
                        }
                    }
                }

                if ds.len() == 1 {
                    // [A] => A
                    ds.remove(0)
                } else {
                    // [A,[B,C],{D,E}] => [A,B,C,{D,E}]
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

        if directed {
            // Adjacent repeats only add a self edge: (A, A, B) => (A, B)
            for seq in seqs.iter_mut() {
                seq.dedup();
            }
        }

        let mut seqs: Vec<Expr> = seqs
            .into_iter()
            .map(|mut seq| match seq.len() {
                0 => Expr::Disconnected(vec![]),
                // {A} => A
                1 => seq.remove(0),
                _ => variant(seq),
            })
            .collect();

        if seqs.len() == 1 {
            seqs.remove(0)
        } else {
            Expr::Disconnected(seqs)
        }
    }

    // This only works on normalized expressions.
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
        let mut fresh: Vec<Expr> = Vec::new();
        for expr in exprs {
            // Skip anything already covered by an existing expr.
            if fresh.iter().any(|f| expr.is_norm_subgraph(f)) {
                continue;
            }
            // Replace every existing expr this one covers.
            fresh.retain(|f| !f.is_norm_subgraph(expr));
            fresh.push(expr.clone());
        }
        fresh
    }

    // Folds a directed expression and its reverse back into a connected one:
    // [(A, B), (B, A)] => [{A, B}]
    fn fold(&self) -> Self {
        match self {
            Expr::Disconnected(exprs) => {
                let mut fresh: Vec<Expr> = vec![];
                for expr in exprs {
                    let rev = expr.transpose();
                    match fresh.iter().position(|f| *f == rev) {
                        Some(i) if matches!(expr, Expr::Directed(_)) => {
                            if let Expr::Directed(ds) = &fresh[i] {
                                fresh[i] = Expr::Connected(ds.clone());
                            }
                        }
                        _ => fresh.push(expr.clone()),
                    }
                }
                Expr::Disconnected(fresh)
            }
            e => e.clone(),
        }
    }

    fn is_norm_subgraph(&self, other: &Self) -> bool {
        let nodes: HashSet<Node> = other.nodes().into_iter().collect();
        let edges: HashSet<(Node, Node)> = other.norm_edges().into_iter().collect();
        self.nodes().iter().all(|node| nodes.contains(node))
            && self.norm_edges().iter().all(|edge| edges.contains(edge))
    }
}

impl Normalize for Expr {
    fn normalize(&self) -> Self {
        self.flatten().dedup().fold().flatten()
    }
}

impl<'src> Normalize for Stmt {
    fn normalize(&self) -> Self {
        match self {
            Stmt::Assign(node, expr) => Stmt::Assign(node.clone(), expr.normalize()),
        }
    }
}

impl Normalize for Ret {
    fn normalize(&self) -> Self {
        let norm_stmts = self.0.iter().map(Normalize::normalize).collect();
        let norm_expr = self.1.normalize();
        Ret(norm_stmts, norm_expr)
    }
}

#[cfg(test)]
mod tests {
    use super::Normalize;
    use crate::{Expr, Parse, Ret, Stmt};
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
        // A repeat carries the edge back, so it can't be dropped.
        assert_eq!(
            Expr::parse("(A, B, A)").unwrap().edges(),
            Expr::parse("{A, B}").unwrap().edges(),
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
            Ret::parser()
                .parse(
                    r#"
                    G1 = {A, [B, C]}
                    [[{{D}}]]
                    "#
                )
                .unwrap()
                .normalize(),
            Ret::parser()
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
