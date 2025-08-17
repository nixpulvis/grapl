use crate::{Expr, Node, Ret, Stmt};
use std::collections::HashMap;

/// Graph resolution configuration options.
#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    shadowing: bool,
    // TODO: This will probably start by looking something like this:
    // ```
    // Config { recursion: Recursion, ... }
    // struct RecursionConfig { limit: usize, ... }
    // struct Recursion { config: &RecursionConfig, depth: usize, ... }
    // struct Env(HashMap, Config, Recursion)
    // ```
    recursion: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // TODO: Is the right default?
            shadowing: false,
            recursion: false,
        }
    }
}

impl Config {
    pub fn with_shadowing(mut self) -> Self {
        self.shadowing = true;
        self
    }

    pub fn with_recursion(mut self) -> Self {
        self.recursion = true;
        self
    }
}

#[derive(Debug)]
/// Errors that can occur during resolution.
pub enum Error {
    /// ```grapl
    /// G = {A, B}
    /// G = {B, C}
    /// ```
    Shadowing,
    /// ```grapl
    /// G = {G, B}
    /// ```
    Recursion,
}

/// Running resolution environment used to maintain state.
#[derive(Debug, PartialEq, Eq)]
pub struct Env<'cfg, 'src>(HashMap<Node<'src>, Expr<'src>>, &'cfg Config);

impl<'cfg, 'src> Env<'cfg, 'src> {
    pub fn new(config: &'cfg Config) -> Self {
        Env(HashMap::new(), config)
    }

    pub fn lookup(&self, node: &Node<'src>) -> Expr<'src> {
        if let Some(expr) = self.0.get(node) {
            expr.clone()
        } else {
            Expr::Node(node.clone())
        }
    }

    pub fn insert(&mut self, node: Node<'src>, expr: Expr<'src>) -> Result<(), Error> {
        if !self.1.shadowing && self.0.contains_key(&node) {
            Err(Error::Shadowing)
        } else if !self.1.recursion && expr.contains_node(&node) {
            Err(Error::Recursion)
        } else {
            self.0.insert(node, expr);
            Ok(())
        }
    }
}

/// Resolution of named graphs in [`Ret`] and slices of [`Stmt`] structures.
///
/// ```grapl
/// G = [A, B]
/// {X, G}
/// =>
/// [{X, A}, {X, B}]
/// ```
pub trait Resolve<'src>
where
    Self: Sized,
{
    type Output;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg, 'src>) -> Result<Self::Output, Error>;
}

impl<'src> Resolve<'src> for Expr<'src> {
    type Output = Self;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg, 'src>) -> Result<Self::Output, Error> {
        macro_rules! inner {
            ($exprs:expr, $variant:path) => {{
                let mut fresh = vec![];
                for expr in $exprs {
                    fresh.push(expr.resolve(env)?);
                }
                Ok($variant(fresh))
            }};
        }

        match self {
            Expr::Node(node) => Ok(env.lookup(node)),
            Expr::Connected(exprs) => inner!(exprs, Expr::Connected),
            Expr::Disconnected(exprs) => inner!(exprs, Expr::Disconnected),
        }
    }
}

impl<'src> Resolve<'src> for Vec<Stmt<'src>> {
    type Output = Self;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg, 'src>) -> Result<Self::Output, Error> {
        let mut fresh = vec![];
        for stmt in self {
            match stmt {
                Stmt::Assign(node, expr) => {
                    let resolved = expr.resolve(env)?;
                    env.insert(node.clone(), resolved.clone())?;
                    fresh.push(Stmt::Assign(node.clone(), resolved));
                }
            }
        }
        Ok(fresh)
    }
}

impl<'src> Resolve<'src> for Ret<'src> {
    type Output = Expr<'src>;

    fn resolve<'cfg>(&self, env: &mut Env<'cfg, 'src>) -> Result<Self::Output, Error> {
        self.0.resolve(env)?;
        self.1.resolve(env)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Expr, Node, Parse, Resolve, Ret, Stmt,
        resolve::{Config, Env},
    };
    use pretty_assertions::assert_eq;

    #[test]
    fn resolve_expr() {
        let config = Config::default();
        let mut env = Env::new(&config);
        env.insert(Node("A"), Expr::Node(Node("B"))).unwrap();

        assert_eq!(
            Expr::parse("A").unwrap().resolve(&mut env).unwrap(),
            Expr::parse("B").unwrap(),
        );
    }

    #[test]
    fn resolve_stmts() {
        let config = Config::default();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = [A, B]
                    G2 = {X, G1}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = [A, B]
                    G2 = {X, [A, B]}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    fn resolve_ret() {
        let config = Config::default();
        let mut env = Env::new(&config);

        assert_eq!(
            Ret::parse(
                r#"
                    G = A
                    A
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Expr::parse("A").unwrap(),
        );
    }

    #[test]
    fn resolve_shadowing() {
        let config = Config::default().with_shadowing();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = A
                    G1 = B
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = A
                    G1 = B
                    G2 = B
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    fn resolve_apparent_recursion_shadowing() {
        let config = Config::default().with_shadowing();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G2
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G = {G, X}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            // TODO: Handle multi-step resolution and proper recursion end
            // conditions.
            Vec::<Stmt>::parse(
                r#"
                    G = {{G..., X}, X}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_mutual_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = {G2, X}
                    G2 = {G1, Y}
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = {{G1..., Y}, X}
                    G2 = {{G2..., X}, Y}
                "#
            )
            .unwrap(),
        );
    }

    #[test]
    #[ignore]
    fn resolve_direct_mutual_recursion() {
        let config = Config::default().with_recursion();
        let mut env = Env::new(&config);

        assert!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .is_err()
        );
    }

    #[test]
    #[ignore]
    fn resolve_direct_mutual_recursion_shadowing() {
        let config = Config::default().with_recursion().with_shadowing();
        let mut env = Env::new(&config);

        // This is going to eventually be an error in one way or another.
        assert_eq!(
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2
                    G2 = G1
                "#
            )
            .unwrap()
            .resolve(&mut env)
            .unwrap(),
            Vec::<Stmt>::parse(
                r#"
                    G1 = G2...
                    G2 = G1...
                "#
            )
            .unwrap(),
        );
    }
}
