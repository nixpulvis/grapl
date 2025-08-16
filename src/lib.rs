use chumsky::prelude::*;

/// Nodes used as base indentifiers or to refer to other graphs.
///
/// Examples of nodes: `A`, `a`, `G1`...
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<'src>(&'src str);

pub fn node<'src>() -> impl Parser<'src, &'src str, Node<'src>> + Clone {
    text::ascii::ident().padded().map(Node)
}

/// Expressions describe a graph.
///
/// ```grapl
/// { A, B }
/// { A, [B, C] }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<'src> {
    Node(Node<'src>),
    Connected(Vec<Expr<'src>>),
    Disconnected(Vec<Expr<'src>>),
    Stmts(Vec<Stmt<'src>>, Box<Expr<'src>>),
}

pub fn expr<'src>() -> impl Parser<'src, &'src str, Expr<'src>> + Clone {
    recursive(|expr| {
        let node = node().map(Expr::Node);

        let seq = expr
            .clone()
            .separated_by(just(",").padded())
            .allow_trailing()
            .collect::<Vec<_>>();

        let connected = seq
            .clone()
            .delimited_by(just('{'), just('}'))
            .map(Expr::Connected);

        let disconnected = seq
            .clone()
            .delimited_by(just('['), just(']'))
            .map(Expr::Disconnected);

        choice((node, connected, disconnected)).padded()
    })
}

/// Statements are a sequence of graph assignments for nested use.
///
/// ```grapl
/// G1 = {A, B}
/// G2 = [C, D]
/// G  = {G1, G2}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt<'src> {
    Assign(Node<'src>, Expr<'src>),
}

pub fn stmts<'src>() -> impl Parser<'src, &'src str, Vec<Stmt<'src>>> + Clone {
    node()
        .then(just("=").padded())
        .then(expr())
        .map(|((n, _), e)| Stmt::Assign(n, e))
        .separated_by(text::whitespace())
        .collect::<Vec<_>>()
}

/// Returns are a sequence of statements followed by a final graph expression.
///
/// ```grapl
/// G = {A, B}
/// {G, [C, D]}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ret<'src>(Vec<Stmt<'src>>, Expr<'src>);

pub fn ret<'src>() -> impl Parser<'src, &'src str, Ret<'src>> + Clone {
    // stmts()
    //     .then(newline())
    //     .then(expr())
    //     .map(|((s, _), e)| Ret(s, e))
    stmts().then(expr()).map(|(s, e)| Ret(s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! node {
        ($($id:ident),*) => {
            $(const $id: Node<'static> = Node(stringify!($id));)*
        };
    }

    #[test]
    fn parse_node() {
        node!(A);

        assert!(node().parse("").has_errors());
        assert!(node().parse("1").has_errors());
        assert_eq!(node().parse("A").into_result(), Ok(A));
    }

    macro_rules! enode {
        ($($id:ident),*) => {
            $(const $id: Expr<'static> = Expr::Node(Node(stringify!($id)));)*
        };
    }

    #[test]
    fn parse_expr() {
        enode!(A, B, C, D);

        assert_eq!(
            expr().parse("{}").into_result(),
            Ok(Expr::Connected(vec![]))
        );
        assert_eq!(
            expr().parse("[]").into_result(),
            Ok(Expr::Disconnected(vec![]))
        );
        assert_eq!(
            expr().parse("{  A }").into_result(),
            Ok(Expr::Connected(vec![A]))
        );
        assert_eq!(
            expr().parse("[A,  B,  ]").into_result(),
            Ok(Expr::Disconnected(vec![A, B]))
        );
        assert_eq!(
            expr()
                .parse(
                    r#"
                        {A, [B, C]}
                "#
                )
                .into_result(),
            Ok(Expr::Connected(vec![A, Expr::Disconnected(vec![B, C])]))
        );
        assert_eq!(
            expr().parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Connected(vec![A, B]),
                Expr::Disconnected(vec![C, D])
            ]))
        );
    }

    #[test]
    fn parse_stmt() {
        node!(G, H, G1, G2);
        enode!(A, B, C, D);

        assert_eq!(stmts().parse("").into_result(), Ok(vec![]),);
        assert_eq!(
            stmts().parse("G = {A, B}").into_result(),
            Ok(vec![Stmt::Assign(G, Expr::Connected(vec![A, B]))]),
        );
        assert_eq!(
            stmts().parse("G = {A, B}H = [C, D]").into_result(),
            Ok(vec![
                Stmt::Assign(G, Expr::Connected(vec![A, B])),
                Stmt::Assign(H, Expr::Disconnected(vec![C, D])),
            ]),
        );
        assert_eq!(
            stmts()
                .parse(
                    r#"
                        G1 = {A, B}

                        G2 = {[G1, C], D}
                "#
                )
                .into_result(),
            Ok(vec![
                Stmt::Assign(G1, Expr::Connected(vec![A, B])),
                Stmt::Assign(
                    G2,
                    Expr::Connected(vec![Expr::Disconnected(vec![Expr::Node(G1), C]), D])
                ),
            ]),
        );
    }

    #[test]
    fn parse_ret() {
        node!(G);
        enode!(A, B, C, D);

        assert_eq!(
            ret().parse("{A, [C, D]}").into_result(),
            Ok(Ret(
                vec![],
                Expr::Connected(vec![A, Expr::Disconnected(vec![C, D])]),
            ))
        );

        assert_eq!(
            ret().parse("G = {A, [C, D]} {G, B}").into_result(),
            Ok(Ret(
                vec![Stmt::Assign(
                    G,
                    Expr::Connected(vec![A, Expr::Disconnected(vec![C, D])])
                )],
                Expr::Connected(vec![Expr::Node(G), B]),
            ))
        );

        assert_eq!(
            expr()
                .parse(
                    r#"
                        {G, [C, D]}
                "#
                )
                .into_result(),
            Ok(Expr::Connected(vec![
                Expr::Node(G),
                Expr::Disconnected(vec![C, D])
            ]))
        );

        assert_eq!(
            ret()
                .parse(
                    r#"
                        G = {A, B}
                        {G, [C, D]}
                "#
                )
                .into_result(),
            Ok(Ret(
                vec![Stmt::Assign(G, Expr::Connected(vec![A, B]))],
                Expr::Connected(vec![Expr::Node(G), Expr::Disconnected(vec![C, D])]),
            ))
        );
    }
}
