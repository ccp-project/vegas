extern crate clap;
use clap::Arg;
extern crate time;
#[macro_use]
extern crate slog;

extern crate ccp_vegas;
extern crate portus;

use ccp_vegas::Vegas;
use portus::ipc::{BackendBuilder, Blocking};

fn make_args() -> Result<(ccp_vegas::VegasConfig, String), String> {
    let matches = clap::App::new("CCP Vegas")
        .version("0.1.0")
        .author("Frank Cangialosi <frankc@csail.mit.edu>")
        .about("Implementation of Vegas congestion control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix|char)")
             .default_value("unix")
             .validator(portus::algs::ipc_valid))
        .arg(Arg::with_name("alpha")
             .long("alpha")
             .help("Increase cwnd if estimate of packets in queue <= alpha")
             .default_value("2"))
        .arg(Arg::with_name("beta")
             .long("beta")
             .help("Decrease cwnd if estimate of packets in queue >= beta")
             .default_value("4"))
        .get_matches();

    Ok((
        ccp_vegas::VegasConfig {
            alpha: matches.value_of("alpha").unwrap().parse::<u32>().unwrap(),
            beta: matches.value_of("beta").unwrap().parse::<u32>().unwrap(),
        },
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

fn main() {
    let log = portus::algs::make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or_default();

    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::<Blocking>::new("in", "out")
                .map(|sk| BackendBuilder{sock: sk})
                .expect("unix ipc initialization");
            portus::run::<_, Vegas<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::<Blocking>::new()
                .map(|sk| BackendBuilder{sock: sk})
                .expect("netlink ipc initialization");
            portus::run::<_, Vegas<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "char" => {
            use portus::ipc::kp::Socket;
            let b = Socket::<Blocking>::new()
                .map(|sk| BackendBuilder {sock: sk})
                .expect("char ipc initialization");
            portus::run::<_, Vegas<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap()
        }
        _ => unreachable!(),
    }
            
}
