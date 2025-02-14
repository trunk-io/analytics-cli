use std::{env, fs, path::Path};

use schemars::schema::{RootSchema, Schema};
use typify::{TypeSpace, TypeSpaceSettings};

fn main() {
    let schema_names_and_top_level_types: &[(&str, &[&str])] = &[
        (
            "xcrun-xcresulttool-get-test-results-tests-json-schema",
            &["Tests"],
        ),
        (
            "xcrun-xcresulttool-formatDescription-get---format-json---legacy-json-schema",
            &["ActionsInvocationRecord", "ActionTestPlanRunSummaries"],
        ),
    ];

    for (schema_name, top_level_type_refs) in schema_names_and_top_level_types {
        let content = fs::read_to_string(format!("./{schema_name}.json")).unwrap();
        let schema = serde_json::from_str::<RootSchema>(&content).unwrap();

        let mut type_space = TypeSpace::new(TypeSpaceSettings::default().with_struct_builder(true));
        type_space.add_ref_types(schema.definitions).unwrap();
        for top_level_type_ref in *top_level_type_refs {
            type_space
                .add_type(&Schema::new_ref(format!("#/$defs/{top_level_type_ref}")))
                .unwrap();
        }

        let contents =
            prettyplease::unparse(&syn::parse2::<syn::File>(type_space.to_stream()).unwrap());

        let mut out_file = Path::new(&env::var("OUT_DIR").unwrap()).to_path_buf();
        out_file.push(format!("{schema_name}.rs"));
        fs::write(out_file, contents).unwrap();
    }
}
