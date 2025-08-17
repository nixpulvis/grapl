use chumsky::prelude::*;

/// Nodes used as base indentifiers or to refer to other graphs.
///
/// Examples of nodes: `A`, `a`, `G1`...
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<'src>(&'src str);

impl<'src> Node<'src> {
    pub fn parser() -> impl Parser<'src, &'src str, Node<'src>> + Clone {
        text::ascii::ident().padded().map(Node)
    }
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
}

impl<'src> Expr<'src> {
    pub fn parser() -> impl Parser<'src, &'src str, Expr<'src>> + Clone {
        recursive(|expr| {
            let node = Node::parser().map(Expr::Node);

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

impl<'src> Stmt<'src> {
    pub fn parser() -> impl Parser<'src, &'src str, Vec<Stmt<'src>>> + Clone {
        Node::parser()
            .then(just("=").padded())
            .then(Expr::parser())
            .map(|((n, _), e)| Stmt::Assign(n, e))
            .separated_by(text::whitespace())
            .collect::<Vec<_>>()
    }
}

/// Returns are a sequence of statements followed by a final graph expression.
///
/// ```grapl
/// G = {A, B}
/// {G, [C, D]}
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ret<'src>(Vec<Stmt<'src>>, Expr<'src>);

impl<'src> Ret<'src> {
    pub fn parser() -> impl Parser<'src, &'src str, Ret<'src>> + Clone {
        Stmt::parser().then(Expr::parser()).map(|(s, e)| Ret(s, e))
    }
}

mod normal;
pub use self::normal::Normalize;

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

        assert!(Node::parser().parse("").has_errors());
        assert!(Node::parser().parse("1").has_errors());
        assert_eq!(Node::parser().parse("A").into_result(), Ok(A));
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
            Expr::parser().parse("{}").into_result(),
            Ok(Expr::Connected(vec![]))
        );
        assert_eq!(
            Expr::parser().parse("[]").into_result(),
            Ok(Expr::Disconnected(vec![]))
        );
        assert_eq!(
            Expr::parser().parse("{  A }").into_result(),
            Ok(Expr::Connected(vec![A]))
        );
        assert_eq!(
            Expr::parser().parse("[A,  B,  ]").into_result(),
            Ok(Expr::Disconnected(vec![A, B]))
        );
        assert_eq!(
            Expr::parser()
                .parse(
                    r#"
                        {A, [B, C]}
                "#
                )
                .into_result(),
            Ok(Expr::Connected(vec![A, Expr::Disconnected(vec![B, C])]))
        );
        assert_eq!(
            Expr::parser().parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Connected(vec![A, B]),
                Expr::Disconnected(vec![C, D])
            ]))
        );
        assert_eq!(
            Expr::parser().parse("{{A, B}, [C, D]}").into_result(),
            Ok(Expr::Connected(vec![
                Expr::Connected(vec![A, B]),
                Expr::Disconnected(vec![C, D])
            ]))
        )
    }

    #[test]
    fn parse_stmt() {
        node!(G, H, G1, G2);
        enode!(A, B, C, D);

        assert_eq!(Stmt::parser().parse("").into_result(), Ok(vec![]),);
        assert_eq!(
            Stmt::parser().parse("G = {A, B}").into_result(),
            Ok(vec![Stmt::Assign(G, Expr::Connected(vec![A, B]))]),
        );
        assert_eq!(
            Stmt::parser().parse("G = {A, B}H = [C, D]").into_result(),
            Ok(vec![
                Stmt::Assign(G, Expr::Connected(vec![A, B])),
                Stmt::Assign(H, Expr::Disconnected(vec![C, D])),
            ]),
        );
        assert_eq!(
            Stmt::parser()
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
            Ret::parser().parse("{A, [C, D]}").into_result(),
            Ok(Ret(
                vec![],
                Expr::Connected(vec![A, Expr::Disconnected(vec![C, D])]),
            ))
        );

        assert_eq!(
            Ret::parser().parse("G = {A, [C, D]} {G, B}").into_result(),
            Ok(Ret(
                vec![Stmt::Assign(
                    G,
                    Expr::Connected(vec![A, Expr::Disconnected(vec![C, D])])
                )],
                Expr::Connected(vec![Expr::Node(G), B]),
            ))
        );

        assert_eq!(
            Ret::parser()
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
