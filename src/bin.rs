use std::fs::read_to_string;
use std::process::ExitCode;
use std::sync::Mutex;
use std::{error::Error, sync::Arc};

use librqbe::ssaify::{ControlFlowGraph, PhiNodeInsertion};
use librqbe::target::CompilationTarget;
use log::error;

static LOCK_STDOUT: Mutex<()> = Mutex::new(());

fn thread_work(path: &str, target: Arc<CompilationTarget>) -> Result<(), Box<dyn Error + Send>> {
    let s = read_to_string(path).expect("could not read file");
    let p = librqbe::parse::Parser::new(&s, target);
    let mut module = match p.parse_module() {
        Ok(it) => it,
        Err(err @ librqbe::parse::ParseError::UnexpectedToken(t)) => {
            error!("\n{}\n{err}", t.span.view_lines_around(&s, 5));
            return Err(Box::new(err));
        }
        Err(err) => {
            error!("{err}");
            return Err(Box::new(err));
        }
    };
    let cfg = match ControlFlowGraph::build(&module) {
        Ok(it) => it,
        Err(err) => {
            error!("{err}\n{}", module.display_module());
            return Err(Box::new(err));
        }
    };
    let pretty_cfg = cfg.dump_cfg_dot(&module.ctx());
    let pretty_domtree = cfg.dump_domtree_dot(&module.ctx());
    let s_before = module.display_module();
    let mut phi_inserter = PhiNodeInsertion::new(cfg);
    let func_keys: Vec<_> = module.functions().keys().collect();
    for f in func_keys {
        phi_inserter.visit_module(f, &mut module);
    }
    let s_after = module.display_module();

    // let locked = LOCK_STDOUT.lock().unwrap();
    println!("before phi\n{s_before}\n\nafter phi:\n{s_after}");
    println!("digraph {{\n{pretty_cfg}\n{pretty_domtree}\n}}");
    // drop(locked);

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    simplelog::SimpleLogger::init(log::LevelFilter::Error, simplelog::Config::default()).unwrap();
    let t = std::sync::Arc::new(CompilationTarget::host());

    // std::thread::scope(|s| {
    //     let mut handles = Vec::with_capacity(args.len());
    //     for path in &args {
    //         let t_clone = Arc::clone(&t);
    //         handles.push(s.spawn(move || thread_work(path, t_clone)));
    //     }
    //     for handle in handles {
    //         let id = handle.thread().id();
    //         if let Err(e) = handle.join().unwrap() {
    //             trace!("thread with {id:?} errored:\n{e}");
    //         }
    //     }
    // });
    for path in &args {
        match thread_work(path, Arc::clone(&t)) {
            Ok(it) => it,
            Err(err) => return Err(err),
        };
    }

    Ok(())
}
