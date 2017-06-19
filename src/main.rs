extern crate clap;
extern crate fibers;
extern crate hb;
#[macro_use]
extern crate trackable;

use std::fs::File;
use std::io;
use clap::{App, Arg, SubCommand};
use fibers::{Executor, ThreadPoolExecutor, Spawn};
use hb::Error;

fn main() {
    let matches = App::new("hb")
        .version(env!("CARGO_PKG_VERSION"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .subcommand(
            SubCommand::with_name("run").arg(
                Arg::with_name("INPUT")
                    .short("i")
                    .long("input")
                    .takes_value(true)
                    .default_value("-"),
            ),
        )
        .get_matches();
    if let Some(matches) = matches.subcommand_matches("run") {
        let requests = match matches.value_of("INPUT").unwrap() {
            "-" => track_try_unwrap!(hb::run::RequestQueue::read_from(io::stdin())),
            filepath => {
                let f = track_try_unwrap!(File::open(filepath).map_err(Error::from));
                track_try_unwrap!(hb::run::RequestQueue::read_from(f))
            }
        };

        let mut executor = track_try_unwrap!(ThreadPoolExecutor::new().map_err(Error::from));
        let runner = hb::run::Runner::new(executor.handle(), requests);
        let monitor = executor.spawn_monitor(runner);
        let result = track_try_unwrap!(executor.run_fiber(monitor).map_err(Error::from));
        let responses = track_try_unwrap!(result.map_err(Error::from));
        println!("{:?}", responses);
    } else {
        println!("Usage: {}", matches.usage());
        std::process::exit(1);
    }
}
