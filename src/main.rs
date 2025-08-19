use chumsky::prelude::*;
#[cfg(feature = "petgraph")]
use grapl::Node;
use grapl::resolve::{Config, Env};
use grapl::{Expr, Normalize, Parse, Resolve, Stmt};
use microxdg::{Xdg, XdgError};
#[cfg(feature = "petgraph")]
use petgraph::{
    dot::{Config as DotConfig, Dot},
    graph::Graph,
};
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{DefaultEditor, Editor};
use std::fs;
#[cfg(feature = "petgraph")]
use std::fs::File;
#[cfg(feature = "petgraph")]
use std::io::prelude::*;
use std::path::PathBuf;

fn main() -> rustyline::Result<()> {
    let mut rl = DefaultEditor::new()?;
    load_history(&mut rl);

    let config = Config::default().with_shadowing();
    let mut env = Env::new(&config);

    loop {
        let readline = rl.readline("> ");
        match readline {
            Ok(line) => {
                handle_line(line, &mut env, &mut rl);
            }
            Err(ReadlineError::Interrupted) => {
                println!("Ctrl-C pressed. Exiting.");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Ctrl-D pressed. Exiting.");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    save_history(&mut rl);

    Ok(())
}

enum Input {
    Expr(Expr),
    Stmt(Stmt),
    Cmd(Cmd),
}

enum Cmd {
    Env,
    #[cfg(feature = "petgraph")]
    Viz(Expr, Option<PathBuf>),
}

fn repl_parser<'src>() -> impl Parser<'src, &'src str, Input> {
    let stmt = Stmt::parser().map(|s| Input::Stmt(s));
    let expr = Expr::parser().map(|e| Input::Expr(e));
    let env = just("!env").padded().map(|_| Input::Cmd(Cmd::Env));
    #[cfg(feature = "petgraph")]
    let viz = just("!viz ")
        .then(Expr::parser())
        .padded()
        .then(any().repeated().collect().map(|p: String| {
            if p == "" {
                None
            } else {
                Some(PathBuf::from(p))
            }
        }))
        .map(|((_, expr), path)| Input::Cmd(Cmd::Viz(expr, path)));

    #[cfg(feature = "petgraph")]
    {
        choice((stmt, expr, env, viz))
    }

    #[cfg(not(feature = "petgraph"))]
    choice((stmt, expr, env))
}

fn handle_line<'cfg, 'src>(line: String, env: &mut Env<'cfg>, rl: &mut Editor<(), FileHistory>) {
    match repl_parser().parse(&line).into_result() {
        Ok(input) => {
            rl.add_history_entry(&line).unwrap();
            match input {
                Input::Expr(expr) => match expr.resolve(env) {
                    Ok(expr) => println!("{}", expr.normalize()),
                    Err(err) => println!("Error: {:?}", err),
                },
                Input::Stmt(stmts) => {
                    if let Err(err) = stmts.resolve(env) {
                        println!("Error: {:?}", err);
                    }
                }
                Input::Cmd(Cmd::Env) => {
                    print!("{}", env);
                }
                #[cfg(feature = "petgraph")]
                Input::Cmd(Cmd::Viz(expr, save)) => match expr.resolve(env) {
                    Ok(resolved) => {
                        handle_viz(&resolved, save);
                    }
                    Err(err) => {
                        println!("Error: {:?}", err);
                    }
                },
            }
        }
        Err(_errors) => {
            println!("Error: Invalid syntax");
            // for error in errors {
            //     println!("{}", error)
            // }
        }
    }
}

#[cfg(feature = "petgraph")]
fn handle_viz(expr: &Expr, save: Option<PathBuf>) {
    let graph: Graph<Node, ()> = expr.into();
    let dot = Dot::with_config(&graph, &[DotConfig::EdgeNoLabel]);
    if let Some(path) = save {
        if let Ok(mut file) = File::create(&path) {
            if file.write_all(format!("{:?}", dot).as_bytes()).is_err() {
                print!("Failed to write to {}", path.display());
            }
        }
    } else {
        print!("{:?}", dot);
    }
}

const HISTDIR: &'static str = "grapl";
const HISTFILE: &'static str = "grapl.history";

fn load_history(rl: &mut Editor<(), FileHistory>) {
    with_histfile(rl, |rl, path| {
        rl.load_history(path).ok();
    });
}

fn save_history(rl: &mut Editor<(), FileHistory>) {
    with_histfile(rl, |rl, path| {
        rl.save_history(path).ok();
    });
}

fn with_histfile<F>(rl: &mut Editor<(), FileHistory>, func: F)
where
    F: Fn(&mut Editor<(), FileHistory>, &PathBuf),
{
    if let Ok(mut path) = get_xdg_state_dir() {
        path.push(HISTDIR);
        if fs::create_dir_all(&path).is_ok() {
            path.push(HISTFILE);
            func(rl, &path)
        }
    }
}

fn get_xdg_state_dir() -> Result<PathBuf, XdgError> {
    let xdg = Xdg::new()?;
    let state_dir = xdg.state()?;
    Ok(state_dir)
}
