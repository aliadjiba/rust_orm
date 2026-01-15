extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(Model)]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Get all field names
    let fields: Vec<_> = match input.data {
        syn::Data::Struct(ref data) => data.fields.iter().collect(),
        _ => panic!("Model can only be derived on structs"),
    };

    let field_consts = fields.iter().map(|f| {
        let ident = &f.ident;
        let name_str = ident.as_ref().unwrap().to_string().to_uppercase();
        let value_str = ident.as_ref().unwrap().to_string();
        quote! {
            pub const #ident: &'static str = #value_str;
        }
    });

    let table_name = name.to_string().to_lowercase();

    let expanded = quote! {
        impl #name {
            #(#field_consts)*
        }

        impl crate::model::Model for #name {
            fn table_name() -> &'static str {
                #table_name
            }
            fn relations(&self) -> &crate::model::Relations {
                &self.relations
            }
            fn relations_mut(&mut self) -> &mut crate::model::Relations {
                &mut self.relations
            }
        }
    };

    TokenStream::from(expanded)
}
