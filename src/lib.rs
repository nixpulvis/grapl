use chumsky::{prelude::*, text::newline};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<'src>(&'src str);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<'src> {
    Node(Node<'src>),
    Connected(Vec<Expr<'src>>),
    Disconnected(Vec<Expr<'src>>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stmt<'src> {
    Assign(Node<'src>, Expr<'src>),
}

pub fn node<'src>() -> impl Parser<'src, &'src str, Node<'src>> + Clone {
    text::ascii::ident().padded().map(Node)
}

pub fn expr<'src>() -> impl Parser<'src, &'src str, Expr<'src>> {
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

        choice((node, connected, disconnected))
    })
}

pub fn stmt<'src>() -> impl Parser<'src, &'src str, Vec<Stmt<'src>>> {
    node()
        .then(just("=").padded())
        .then(expr())
        .map(|((n, _), e)| Stmt::Assign(n, e))
        .separated_by(newline())
        .collect::<Vec<_>>()
        .padded()
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
            expr().parse("[{A,B},[C, D]]").into_result(),
            Ok(Expr::Disconnected(vec![
                Expr::Connected(vec![A, B]),
                Expr::Disconnected(vec![C, D])
            ]))
        );
    }

    #[test]
    fn parse_stmt() {
        node!(G, G1, G2);
        enode!(A, B, C, D);

        assert_eq!(
            stmt().parse("G = {A, B}").into_result(),
            Ok(vec![Stmt::Assign(G, Expr::Connected(vec![A, B]))]),
        );
        assert_eq!(
            stmt()
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
}
