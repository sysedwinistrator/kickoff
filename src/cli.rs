use clap::{crate_authors, crate_version, App, Arg, ArgMatches};

pub fn get_matches() -> ArgMatches<'static> {
    App::new("kickoff")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Minimalistic program launcher")
        .arg(
            Arg::with_name("PROVIDER")
                .long("provider")
                .short("p")
                .takes_value(true)
                .multiple(true)
                .help("Which sources are used to build the list")
                .possible_values(&["path", "stdin", "dot-desktop"])
                .default_value("path"),
        )
        .arg(
            Arg::with_name("CONSUMER")
                .long("consumer")
                .short("c")
                .help("How the selected result will be handled")
                .possible_values(&["stdout", "exec"])
                .default_value("exec"),
        )
        .get_matches()
}
