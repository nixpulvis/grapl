use std::collections::HashSet;

use crate::{Expr, Ret, Stmt};

/// Reductions to normal form.
///
/// - Empty graphs are fully disconnected: `{} => []`
/// - Single node graphs are just the node: `[{A}] => A`
/// - Fully connected graphs are flattened: `{{A, B}, C} => {A, B, C}`
/// - Normalized graphs with disconnected expressions all collapse into a single
///   fully disconnected expression:
/// ```grapl
/// {[A, B], [C, D]} =>
/// [{A, C}, {A, D}, {B, C}, {B, D}]
/// ```
pub trait Normalize: Sized {
    fn normalize(&self) -> Self;
}

impl Expr {
    fn flatten(&self) -> Self {
        match self {
            Expr::Node(node) => Expr::Node(node.clone()),
            Expr::Connected(exprs) => {
                // General reduction strategy follow these steps:
                // {A, [B, C], D, [E, F]} =>
                // [{A}, [B, C], D, [E, F]] =>
                // [{A, B}, {A, C}, D, [E, F]] =>
                // [{A, B, D}, {A, C, D}, [E, F]] =>
                // [{A, B, D, E}, {A, B, D, F}, {A, C, D, E}, {A, C, D, F}]

                // Collect a list of disconnected connected nodes.
                let mut dcs = vec![];
                for expr in exprs {
                    // dcs = []
                    // dcs <= [[]]
                    if dcs.is_empty() {
                        dcs.push(vec![]);
                    }

                    match expr.normalize() {
                        // dcs = [[A],[B]]
                        // expr = C
                        // dcs <= [[A,C],[B,C]]
                        e @ Expr::Node(_) => {
                            for dc in dcs.iter_mut() {
                                dc.push(e.clone());
                            }
                        }
                        // dcs = [[A],[B]]
                        // expr = {C,D}
                        // dcs <= [[A,C,D],[B,C,D]]
                        Expr::Connected(cexprs) => {
                            for cexpr in cexprs {
                                for dc in dcs.iter_mut() {
                                    dc.push(cexpr.clone());
                                }
                            }
                        }
                        // dcs = [[A,B][C]]
                        // expr = [D,E]
                        // dcs <= [[A,B,D],[C,D],[A,B,E],[C,E]]
                        Expr::Disconnected(dexprs) => {
                            let mut freshs = vec![];
                            for dc in dcs.iter() {
                                for dexpr in dexprs.iter() {
                                    let mut fresh = dc.clone();
                                    match dexpr {
                                        // This is kinda gnarly... but we need
                                        // to flatten connected expressions
                                        // inside disconnected expression. E.g:
                                        // {A,[{B,C},D]}.
                                        e @ Expr::Node(_) => fresh.push(e.clone()),
                                        Expr::Connected(cs) => {
                                            for c in cs {
                                                fresh.push(c.clone());
                                            }
                                        }
                                        // This subexpression is normalized and
                                        // therefore cannot have nested [[]].
                                        Expr::Disconnected(_) => unreachable!(),
                                    }
                                    freshs.push(fresh.clone());
                                }
                            }
                            dcs = freshs;
                        }
                    }
                }

                if dcs.len() == 1 {
                    let mut cs = dcs.remove(0);
                    if cs.len() == 1 {
                        // {A} => {A}
                        cs.remove(0)
                    } else {
                        // {[{A, B}]} => {A, B}
                        Expr::Connected(cs)
                    }
                } else {
                    Expr::Disconnected(dcs.into_iter().map(Expr::Connected).collect())
                }
            }
            Expr::Disconnected(exprs) => {
                // Collect a list of disconnected nodes.
                let mut ds = vec![];
                for expr in exprs {
                    match expr.normalize() {
                        // ds = [A,B]
                        // expr = {C,D}
                        // ds <= [A,B,{C,D}]
                        e @ Expr::Node(_) | e @ Expr::Connected(_) => ds.push(e),
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

    // This only works on normalized expressions.
    fn dedup(&self) -> Self {
        macro_rules! dedup_exprs {
            ($varient:path, $exprs:expr) => {{
                let mut fresh = Vec::new();
                for expr in $exprs {
                    enum Action {
                        Insert,
                        Swap,
                        Skip,
                    }
                    let mut action = Action::Insert;
                    for f in fresh.iter() {
                        if expr.is_norm_subgraph(&f) {
                            action = Action::Skip;
                        } else if f.is_norm_subgraph(expr) {
                            action = Action::Swap;
                        }
                    }
                    match action {
                        Action::Insert => {
                            fresh.push(expr.clone());
                        }
                        Action::Swap => {
                            fresh.remove(fresh.len() - 1);
                            fresh.push(expr.clone());
                        }
                        Action::Skip => {}
                    }
                }
                $varient(fresh)
            }};
        }
        match self {
            e @ Expr::Node(_) => e.clone(),
            Expr::Connected(exprs) => dedup_exprs!(Expr::Connected, exprs),
            Expr::Disconnected(exprs) => dedup_exprs!(Expr::Disconnected, exprs),
        }
    }

    fn is_norm_subgraph(&self, other: &Self) -> bool {
        let set: HashSet<_> = other.nodes().iter().cloned().collect();
        self.nodes().iter().all(|node| set.contains(node))
    }
}

impl Normalize for Expr {
    fn normalize(&self) -> Self {
        self.flatten().dedup().flatten()
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
            Expr::parse("[N,I,{N,J}]").unwrap(),
        );
        assert_eq!(
            Expr::parse("[N,I,{N,J}]").unwrap().dedup(),
            Expr::parse("[I,{N,J}]").unwrap(),
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
