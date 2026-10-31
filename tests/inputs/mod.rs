//! The real-data inputs this crate's tests read, each with what it is and where it comes from
//! (`primase_testdata::inputs!`; README.md here lists them). A test whose input is not configured skips with a line
//! naming its key. An input declared `gate: true` is one a complete run needs: with `[run] require = 'gate'` in the
//! `GAME_DATA` file, a missing one fails the test instead of skipping it. Integration tests reach this module with
//! `mod inputs;`, unit tests with `#[cfg(test)] #[path = "../tests/inputs/mod.rs"] mod inputs;` in the crate root, and
//! tests/real_data_inputs.rs checks that every query the crate's sources write is declared here. Declare an input
//! here when a test first asks for it.

primase_testdata::inputs! {
    games {
        Fh6 = "fh6",
    }
    installs {
        Fh6 {
            about: "Forza Horizon 6's install folder (the folder that holds `media`)",
            source: "the user's own install",
            gate: false,
        },
    }
    keys {
        Fh6 {
            about: "the user's Forza Horizon 6 container keys: a text file in the layout `keys::Keys::parse` reads, with the usages `GameDB` and `General` and `gamedb.scramble`",
            source: "the user's own copy of the keys; none is distributed here",
            gate: false,
        },
    }
    tools {}
    oracle_sets {}
    declared {
        Query::Install(Game::Fh6, Some("media/Stripped/gamedbRC.slt")) => {
            status: Unheld("Forza Horizon 6 is not installed on any machine that runs these tests yet"),
            gate: false,
            about: r#"the game database: a large-block container, read by tests/real_data.rs to an SQLite image"#,
            source: r#"the user's own install"#,
        },
        Query::Install(Game::Fh6, Some("media/NerdData/NerdData.json")) => {
            status: Unheld("Forza Horizon 6 is not installed on any machine that runs these tests yet"),
            gate: false,
            about: r#"a one-block small container, read by tests/real_data.rs with every MAC checked"#,
            source: r#"the user's own install"#,
        },
    }
    debug {}
}
