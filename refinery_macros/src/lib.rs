//! Contains Refinery macros
//!
//! used to import and embed migration files
#![recursion_limit = "128"]
extern crate proc_macro;

mod util;

use proc_macro::TokenStream;
use proc_macro2::{Span as Span2, TokenStream as TokenStream2};
use quote::quote;
use quote::ToTokens;
use std::ffi::OsStr;
use syn::{parse_macro_input, Ident, LitStr};

use util::{crate_root, find_migration_files, MigrationType};

fn migration_fn_quoted<T: ToTokens>(_migrations: Vec<T>) -> TokenStream2 {
    let result = quote! {
        use refinery::{Migration, Runner};
        pub fn runner() -> Runner {
            let quoted_migrations: Vec<(&str, String)> = vec![#(#_migrations),*];
            let mut migrations: Vec<Migration> = Vec::new();
            for module in quoted_migrations.into_iter() {
                migrations.push(Migration::from_filename(module.0, &module.1).unwrap());
            }
            Runner::new(&migrations)
        }
    };
    result
}

/// imports rust migration modules with migrations and inserts a function called runner that when called returns a [Runner](refinery_migrations::Runner) instance with the collected migration modules.
///
/// `include_migration_mods` expects to be called from a `mod.rs` file in directory called migrations below the src directory of your Rust project.
/// if you want the directory to have another name you have to call `include_migration_mods` with it's path relative to the crate root.
/// In the future this will not be needed and `include_migration_mods` will detect automatically from which module it is being called.
///
/// To be a valid migration module, it has to be named in the format `V{1}__{2}.rs ` where `{1}` represents the migration version and `{2}` the name.
/// For the name alphanumeric characters plus "_"  are supported.
/// the migration module must have a function named migration that returns a [`String`](https://doc.rust-lang.org/std/string/struct.String.html)
/// # Example using [Barrel](https://docs.rs/barrel/)
/// ```ignore
/// // module named V1__add_persons_table.rs in src/db/migrations
/// use barrel::backend::MySql;
/// use barrel::{Migration, types};
///
/// pub fn migration() -> String {
///    let mut m = Migration::new();
///
///    m.create_table("persons", |t| {
///        t.add_column("id", types::primary());
///        t.add_column("name", types::varchar(255));
///        t.add_column("city", types::varchar(255));
///    });
///
///    m.make::<MySql>()
/// }
/// ```
#[proc_macro]
pub fn include_migration_mods(input: TokenStream) -> TokenStream {
    let location = if input.is_empty() {
        crate_root().join("src").join("migrations")
    } else {
        let location: LitStr = parse_macro_input!(input);
        crate_root().join(location.value())
    };

    let migration_mod_names = find_migration_files(location, MigrationType::Mod)
        .expect("error getting migration files")
        .filter_map(|entry| entry.file_stem().and_then(OsStr::to_str).map(String::from));

    let mut migrations_mods = Vec::new();
    let mut _migrations = Vec::new();

    for migration in migration_mod_names {
        let ident = Ident::new(migration.as_str(), Span2::call_site());
        let mig_mod = quote! {pub mod #ident;};
        _migrations.push(quote! {(#migration, #ident::migration())});
        migrations_mods.push(mig_mod);
    }

    let fnq = migration_fn_quoted(_migrations);
    let result = quote! {
        #(#migrations_mods)*

        #fnq
    };
    result.into()
}

/// embeds sql migration files and inserts a function called runner that when called returns a [Runner](refinery_migrations::Runner) instance with the collected migration files
///
/// when called without arguments `embed_migrations` searches for migration files on a directory called `migrations` at the root level of your crate.
/// if you want to specify another directory call `embed_migrations!` with it's location relative to the root level of your crate.
///
/// To be a valid migration module, it has to be named in the format `V{1}__{2}.sql ` where `{1}` represents the migration version and `{2}` the name.
/// For the name alphanumeric characters plus "_"  are supported.
/// the migration file must have valid sql instructions for the database you want it to run on.
#[proc_macro]
pub fn embed_migrations(input: TokenStream) -> TokenStream {
    let location = if input.is_empty() {
        crate_root().join("migrations")
    } else {
        let location: LitStr = parse_macro_input!(input);
        crate_root().join(location.value())
    };

    let migration_files =
        find_migration_files(location, MigrationType::Sql).expect("error getting migration files");

    let mut _migrations = Vec::new();
    for path in migration_files {
        let sql = std::fs::read_to_string(path.as_path())
            .unwrap_or_else(|_| panic!("could not read migration {} content", path.display()));
        //safe to call unwrap as find_migration_filenames returns canonical paths
        let filename = path
            .file_stem()
            .and_then(|file| file.to_os_string().into_string().ok())
            .unwrap();
        _migrations.push(quote! {(#filename, #sql.into())});
    }

    let fnq = migration_fn_quoted(_migrations);
    (quote! {
        pub mod migrations {
            #fnq
        }
    })
    .into()
}

#[cfg(test)]
mod tests {
    use super::{migration_fn_quoted, quote};

    #[test]
    fn test_quote_fn() {
        let migs = vec![quote!("V1__first", "valid_sql_file")];
        let expected = concat! {
            "use refinery :: { Migration , Runner } ; ",
            "pub fn runner ( ) -> Runner { ",
            "let quoted_migrations : Vec < ( & str , String ) > = vec ! [ \"V1__first\" , \"valid_sql_file\" ] ; ",
            "let mut migrations : Vec < Migration > = Vec :: new ( ) ; ",
            "for module in quoted_migrations . into_iter ( ) { ",
            "migrations . push ( Migration :: from_filename ( module . 0 , & module . 1 ) . unwrap ( ) ) ; ",
            "} ",
            "Runner :: new ( & migrations ) }"
        };
        assert_eq!(expected, migration_fn_quoted(migs).to_string());
    }
}
