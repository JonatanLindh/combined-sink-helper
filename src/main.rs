use std::process::Command;

use dialoguer::{console::style, theme::ColorfulTheme, Input, MultiSelect, Select};
use itertools::Itertools;
use nom::{
    branch,
    bytes::complete::{tag, take_until1},
    character::complete::{self, not_line_ending},
    combinator,
    multi::many1,
};

#[derive(Debug)]
struct Sink {
    // index: u32,
    name: String,
    description: String,
    owner_module: u32,
    // mute: bool,
    volume: u8,
}

fn take_until_and<'a>(t: &str, i: &'a str) -> nom::IResult<&'a str, String> {
    let (i, a) = take_until1(t)(i)?;
    let (i, b) = tag(t)(i)?;
    let mut r = String::from(a);
    r.push_str(b);

    Ok((i, r))
}

fn parse_sinks(i: &str) -> nom::IResult<&str, Vec<Sink>> {
    let (i, sinks) = many1(parse_sink)(i)?;

    Ok((i, sinks))
}

fn parse_sink(i: &str) -> nom::IResult<&str, Sink> {
    let (i, _) = tag("Sink #")(i)?;
    // let (i, index) = complete::u32(i)?;

    let (i, _) = take_until_and("Name: ", i)?;
    let (i, name) = take_until1("\n")(i)?;

    let (i, _) = take_until_and("Description: ", i)?;
    let (i, description) = take_until1("\n")(i)?;

    let (i, _) = take_until_and("Owner Module: ", i)?;
    let (i, owner_module) = complete::u32(i)?;

    // let (i, _) = take_until_and("Mute: ", i)?;
    // let (i, mute) = take_until1("\n")(i)?;
    // let mute = mute != "no";

    let (i, _) = take_until_and("/  ", i)?;
    let (i, volume) = complete::u8(i)?;

    let (i, _) = branch::alt((take_until1("Sink #"), combinator::rest))(i)?;

    Ok((
        i,
        Sink {
            // index,
            name: String::from(name),
            description: String::from(description),
            owner_module,
            // mute,
            volume,
        },
    ))
}

fn get_sinks() -> Vec<Sink> {
    let out = Command::new("pactl")
        .arg("list")
        .arg("sinks")
        .output()
        .expect("Failed to get sink information");

    let out_string = String::from_utf8(out.stdout).unwrap();
    parse_sinks(&out_string).unwrap().1
}

fn parse_slaves<'a>(i: &'a str, id: &str) -> nom::IResult<&'a str, &'a str> {
    let (i, _) = take_until1(id)(i)?;
    let (i, _) = take_until_and("slaves=", i)?;
    let (i, slaves) = not_line_ending(i)?;

    Ok((i, slaves))
}

fn get_slaves(id: String) -> Vec<String> {
    let e = "Failed to get module information";

    let out = Command::new("pactl")
        .arg("list")
        .arg("modules")
        .output()
        .expect(e);

    let out_string = String::from_utf8(out.stdout).unwrap();

    let s = parse_slaves(&out_string, &id)
        .expect(e)
        .1
        .split(',')
        .map(|s| String::from(s))
        .collect::<Vec<_>>();

    s
}

fn main() {
    match {
        let options = &[
            "Create combined sink",
            "Change volume of slave devices",
            "Remove combined sink",
        ];

        Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What to do?")
            .items(options)
            .default(0)
            .interact()
            .unwrap()
    } {
        0 => create(),
        1 => volume(),
        2 => remove(),
        _ => panic!("Should not be possible"),
    }
}

fn create() {
    let sinks = get_sinks();
    let options = sinks
        .iter()
        .map(|s| s.description.clone())
        .collect::<Vec<_>>();

    let theme = ColorfulTheme {
        unchecked_item_prefix: style("◉".to_string()).for_stderr().black(),
        checked_item_prefix: style("◉".to_string()).for_stderr().green(),
        ..Default::default()
    };

    let selections = MultiSelect::with_theme(&theme)
        .with_prompt("Select sinks to combine")
        .items(&options)
        .interact()
        .unwrap();

    let slaves = sinks
        .iter()
        .enumerate()
        .filter(|(i, _)| selections.contains(i))
        .map(|(_, sink)| sink.name.clone())
        .join(",");

    Command::new("pactl")
        .args([
            "load-module",
            "module-combine-sink",
            "sink_name=\"Combined\"",
            &format!("slaves={slaves}"),
        ])
        .output()
        .expect("Failed to create combined sink");

    println!("\nSuccessfully created combined sink!")
}

fn remove() {
    let id = get_sinks()
        .iter()
        .find(|s| s.name == "Combined")
        .expect("No sink named \"Combined\"")
        .owner_module
        .to_string();

    Command::new("pactl")
        .args(["unload-module", &id])
        .output()
        .expect("Could not remove the combined sink");

    println!("\nSuccessfully removed combined sink!")
}

fn volume() {
    let sinks = get_sinks();
    let id = sinks
        .iter()
        .find(|s| s.name == "Combined")
        .expect("No sink named \"Combined\"")
        .owner_module
        .to_string();

    println!("\nEnter values between 0 and 100. The default is the current volume percentage.");

    get_slaves(id).iter().for_each(|slave| {
        let sink = sinks
            .iter()
            .find(|s| &s.name == slave)
            .expect("Could not find slave sink")
            .clone();

        let new_vol: u8 = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(sink.description.clone())
            .validate_with(|s: &String| -> Result<(), &str> {
                let e = Err("Volume has to be a number between 0 and 100");
                return match s.parse::<u8>() {
                    Err(_) => e,
                    Ok(n) => {
                        if n <= 100 {
                            Ok(())
                        } else {
                            e
                        }
                    }
                };
            })
            .default(sink.volume.to_string())
            .interact_text()
            .unwrap()
            .parse::<u8>()
            .unwrap();

        if new_vol != sink.volume {
            Command::new("pactl")
                .args(["set-sink-volume", slave, &format!("{new_vol}%")])
                .output()
                .expect("Failed to change volume");
        }
    });
}
