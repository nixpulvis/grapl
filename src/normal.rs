use crate::{Expr, Ret, Stmt};

/// Reductions to normal form.
///
/// - Empty graphs are fully disconnected: `{} => []`
/// - Single node graphs are just the node: `[{A}] => A`
/// - Fully connected graphs are flattened: `{{A, B}, C} => {A, B, C}`
/// - Normalized graphs with disconnected expressions all collapse into a single
///   fully disconnected expression:
/// ```
/// {[A, B], [C, D]} =>
/// [{A, C}, {A, D}, {B, C}, {B, D}]
/// ```
pub trait Normalize {
    fn normalize(&self) -> Self;
}

impl<'src> Normalize for Expr<'src> {
    fn normalize(&self) -> Self {
        match self {
            Expr::Node(node) => Expr::Node(node.clone()),
            Expr::Connected(exprs) => {
                // General reduction strategy follow these steps:
                // {A, [B, C], D, [E, F]} =>
                // [{A}, [B, C], D, [E, F]] =>
                // [{A, B}, {A, C}, D, [E, F]] =>
                // [{A, B, D}, {A, C, D}, [E, F]] =>
                // [{A, B, D, E}, {A, C, D, E}, {A, B, D, F}, {A, C, D, F}]

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
                            for dexpr in dexprs.iter() {
                                for dc in dcs.iter() {
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
}

impl<'src> Normalize for Stmt<'src> {
    fn normalize(&self) -> Self {
        match self {
            Stmt::Assign(node, expr) => Stmt::Assign(node.clone(), expr.normalize()),
        }
    }
}

impl<'src> Normalize for Ret<'src> {
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
                    {A, D, E, F},
                    {A, B, C, E, G},
                    {A, D, E, G},
                    ]"
                )
                .unwrap(),
        );
    }

    #[test]
    fn normalize_disjoint_stmts() {
        assert_eq!(
            Stmt::parser()
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
            Stmt::parser()
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
    #[ignore]
    fn normalize_referent_stmt() {
        assert_eq!(
            Stmt::parser()
                .parse(
                    r#"
                    G1 = [A, B]
                    G2 = {X, G1}
                    "#
                )
                .unwrap()
                .iter()
                .map(Normalize::normalize)
                .collect::<Vec<_>>(),
            Stmt::parser()
                .parse(
                    r#"
                    G1 = [A, B]
                    G2 = [{X, A}, {X, B}]
                "#
                )
                .unwrap(),
        );
        // TODO: Consider this case a little more.
        assert_eq!(
            Stmt::parser()
                .parse(
                    r#"
                    G1 = G2
                    G2 = G1
                    "#
                )
                .unwrap()
                .iter()
                .map(Normalize::normalize)
                .collect::<Vec<_>>(),
            Stmt::parser()
                .parse(
                    r#"
                    G1 = G2
                    G2 = G2
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
