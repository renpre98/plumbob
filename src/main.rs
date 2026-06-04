use anyhow::{bail, Result};
use plumbob::{effects, Plumbob};
use std::time::Duration;

fn parse_color(s: &str) -> Result<(u8, u8, u8)> {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16)?;
        let g = u8::from_str_radix(&s[2..4], 16)?;
        let b = u8::from_str_radix(&s[4..6], 16)?;
        return Ok((r, g, b));
    }
    bail!("expected #RRGGBB, got: {s}");
}

fn print_usage() {
    eprintln!(
        "usage:\n  \
         plumbob <R> <G> <B>      set color (0-255 each)\n  \
         plumbob #RRGGBB          set color (hex)\n  \
         plumbob off              turn off\n  \
         plumbob demo             cycle through Sims mood colors with fades\n  \
         plumbob fade #RRGGBB <ms>  fade from current state to a target color"
    );
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut bob = Plumbob::open()?;

    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["off"] => bob.off()?,
        ["demo"] => effects::mood_cycle(
            &mut bob,
            Duration::from_millis(2500),
            Duration::from_millis(800),
        )?,
        ["fade", hex, ms] => {
            let to = parse_color(hex)?;
            let dur = Duration::from_millis(ms.parse()?);
            effects::fade(&mut bob, (0, 0, 0), to, dur)?;
        }
        [hex] => {
            let (r, g, b) = parse_color(hex)?;
            bob.set_rgb(r, g, b)?;
        }
        [r, g, b] => {
            bob.set_rgb(r.parse()?, g.parse()?, b.parse()?)?;
        }
        _ => {
            print_usage();
            std::process::exit(2);
        }
    }
    Ok(())
}
