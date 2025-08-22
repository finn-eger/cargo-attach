use argh::FromArgs;

#[derive(Debug, FromArgs)]
#[doc = "Run `probe-rs attach` with the most recently modified binary or
example for the current package."]
pub struct Args {
    #[argh(switch, short = 'r')]
    #[doc = "only consider release builds"]
    pub(crate) release: bool,

    #[argh(switch, short = 'd')]
    #[doc = "only consider debug builds"]
    pub(crate) debug: bool,

    #[argh(option, arg_name = "TRIPLE")]
    #[doc = "only consider builds for the given target triple"]
    pub(crate) target: Option<String>,

    #[argh(option, arg_name = "NAME")]
    #[doc = "attach to the named binary"]
    pub(crate) bin: Option<String>,

    #[argh(option, arg_name = "NAME")]
    #[doc = "attach to the named example"]
    pub(crate) example: Option<String>,

    #[argh(positional, greedy)]
    #[doc = "arguments to pass to probe-rs"]
    pub(crate) probe_args: Vec<String>,
}
