use clap::{builder::Command, Arg};
use portus::Error;
use tracing::{info, warn};

fn make_args() -> Result<(ccp_vegas::VegasConfig, String), String> {
    let matches = Command::new("CCP Vegas")
        .version("0.1.0")
        .author("Frank Cangialosi <frankc@csail.mit.edu>")
        .about("Implementation of Vegas congestion control")
        .arg(
            Arg::new("ipc")
                .long("ipc")
                .help("Sets the type of ipc to use: (netlink|unix|char)")
                .default_value("unix")
                .value_parser(|s: &str| match portus::algs::ipc_valid(s.to_owned()) {
                    Ok(_) => Ok(s.to_owned()),
                    Err(err) => Err(err),
                }),
        )
        .arg(
            Arg::new("alpha")
                .long("alpha")
                .help("Increase cwnd if estimate of packets in queue <= alpha")
                .default_value("2"),
        )
        .arg(
            Arg::new("beta")
                .long("beta")
                .help("Decrease cwnd if estimate of packets in queue >= beta")
                .default_value("4"),
        )
        .get_matches();

    Ok((
        ccp_vegas::VegasConfig {
            alpha: *matches.get_one("alpha").unwrap(),
            beta: *matches.get_one("beta").unwrap(),
        },
        matches.get_one::<String>("ipc").unwrap().to_owned(),
    ))
}

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    let (cfg, ipc) = make_args()
        .map_err(|err| warn!(?err, "bad argument"))
        .unwrap_or_default();

    info!(algorithm = "Vegas", ?ipc, ?cfg, "starting vegas-ccp");
    portus::start!(ipc.as_str(), cfg)
}
