//! `TrEncodable` 的代码生成。

use zyn::{
    TokenStream, format_ident,
    quote::quote,
    syn::{self, parse_quote},
};

use crate::model_::{FieldModel, FieldsModel, Kind, TypeModel, VariantModel};

pub(crate) fn expand_(model: &TypeModel) -> TokenStream {
    let ident = &model.ident_;
    let type_name = ident.to_string();
    let type_name = type_name.trim_start_matches("r#");
    let lower = type_name.to_lowercase();
    let op_prefix = format_ident!("__DencoEncode{type_name}Op");
    let op_fn = format_ident!("__denco_encode_{lower}_op_async");
    let op_async = format_ident!("{op_prefix}Async");

    let tr_cancellation = format_ident!("__DencoEncTrCancellationToken{type_name}");
    let tr_may_cancel = format_ident!("__DencoEncTrMayCancel{type_name}");
    let tr_encoder = format_ident!("__DencoEncTrEncoder{type_name}");
    let tr_encode_map = format_ident!("__DencoEncTrEncodeMap{type_name}");
    let tr_encode_seq = format_ident!("__DencoEncTrEncodeSeq{type_name}");
    let tr_encode_struct = format_ident!("__DencoEncTrEncodeStruct{type_name}");
    let tr_encode_struct_variant = format_ident!("__DencoEncTrEncodeStructVariant{type_name}");
    let tr_encode_tuple = format_ident!("__DencoEncTrEncodeTuple{type_name}");
    let tr_encode_tuple_struct = format_ident!("__DencoEncTrEncodeTupleStruct{type_name}");
    let tr_encode_tuple_variant = format_ident!("__DencoEncTrEncodeTupleVariant{type_name}");

    let helper = encode_helper_(model, &op_prefix, &op_fn);
    let impl_ = encode_impl_(model, &op_async);

    quote! {
        #[allow(unused_imports)]
        use abs_cancel::{
            TrCancellationToken as #tr_cancellation,
            TrMayCancel as #tr_may_cancel,
        };
        #[allow(unused_imports)]
        use abs_denco::ser::{
            TrEncoder as #tr_encoder,
            TrEncodeMap as #tr_encode_map,
            TrEncodeSeq as #tr_encode_seq,
            TrEncodeStruct as #tr_encode_struct,
            TrEncodeStructVariant as #tr_encode_struct_variant,
            TrEncodeTuple as #tr_encode_tuple,
            TrEncodeTupleStruct as #tr_encode_tuple_struct,
            TrEncodeTupleVariant as #tr_encode_tuple_variant,
        };

        #helper
        #impl_
    }
}

fn encode_helper_(
    model: &TypeModel,
    op_prefix: &syn::Ident,
    op_fn: &syn::Ident,
) -> TokenStream {
    let ident = &model.ident_;
    let ty_generics = model.ty_generics_();
    let type_params = model.type_params_();
    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('a));
    generics.params.push(parse_quote!(S));
    generics.params.push(parse_quote!(C));
    {
        let where_clause = generics.make_where_clause();
        for param in &type_params {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::ser::TrEncodable));
        }
        where_clause
            .predicates
            .push(parse_quote!(S: abs_denco::ser::TrEncoder + 'a));
        where_clause.predicates.push(parse_quote!(S::Ok: 'a));
        where_clause.predicates.push(parse_quote!(S::Error: 'a));
        where_clause
            .predicates
            .push(parse_quote!(C: abs_cancel::TrCancellationToken));
    }
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let body = encode_body_(model);
    let vis = &model.vis_;

    quote! {
        #[gen_mcf2::gen_may_cancel_future(#op_prefix)]
        #[doc(hidden)]
        #[allow(non_snake_case, clippy::all, missing_docs)]
        #vis async fn #op_fn #fn_generics (
            value: &'a #ident #ty_generics,
            encoder: S,
            cancel: C,
        ) -> ::core::result::Result<S::Ok, S::Error>
        #where_clause
        {
            #body
        }
    }
}

fn encode_impl_(model: &TypeModel, op_async: &syn::Ident) -> TokenStream {
    let ident = &model.ident_;
    let ty_generics = model.ty_generics_();
    let type_params = model.type_params_();
    let const_params: Vec<_> = model
        .generics_
        .const_params()
        .map(|param| param.ident.clone())
        .collect();
    let mut generics = model.generics_.clone();
    {
        let where_clause = generics.make_where_clause();
        for param in &type_params {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::ser::TrEncodable));
        }
    }
    let (impl_generics, _, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics abs_denco::ser::TrEncodable for #ident #ty_generics #where_clause {
            type EncodeAsync<'a, S>
                = #op_async<'a, 'a, #(#type_params,)* S, #(#const_params,)*>
            where
                Self: 'a,
                S: abs_denco::ser::TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a;

            fn encode_async<'a, S>(&'a self, encoder: S) -> Self::EncodeAsync<'a, S>
            where
                Self: 'a,
                S: abs_denco::ser::TrEncoder + 'a,
                S::Ok: 'a,
                S::Error: 'a,
            {
                #op_async::new(self, encoder)
            }
        }
    }
}

fn encode_body_(model: &TypeModel) -> TokenStream {
    match &model.kind_ {
        Kind::Struct(FieldsModel::Unit) => {
            let name = &model.rename_;
            quote! {
                encoder
                    .encode_unit_struct(#name)
                    .may_cancel_with(cancel)
                    .await
            }
        }
        Kind::Struct(FieldsModel::Unnamed(fields)) if fields.len() == 1 => {
            let name = &model.rename_;
            if model.transparent_ {
                quote! {
                    abs_denco::ser::TrEncodable::encode_async(&value.0, encoder)
                        .may_cancel_with(cancel)
                        .await
                }
            } else {
                quote! {
                    encoder
                        .encode_newtype_struct(#name, &value.0)
                        .may_cancel_with(cancel)
                        .await
                }
            }
        }
        Kind::Struct(FieldsModel::Unnamed(fields)) => encode_tuple_body_(&model.rename_, fields),
        Kind::Struct(FieldsModel::Named(fields)) => encode_struct_body_(&model.rename_, fields),
        Kind::Enum(variants) => encode_enum_body_(&model.rename_, &model.ident_, variants),
    }
}

fn encode_struct_body_(name: &str, fields: &[FieldModel]) -> TokenStream {
    let len = fields
        .iter()
        .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
        .count();
    let mut body = quote! {
        let mut st = encoder
            .encode_struct(#name, #len)
            .may_cancel_with(cancel.child_token())
            .await?;
    };
    for field in fields {
        let field_name = &field.name_;
        if field.skip_encoding_ || field.is_phantom_ {
            body.extend(quote! {
                st.skip_field(#field_name)
                    .may_cancel_with(cancel.child_token())
                    .await?;
            });
        } else {
            let member = &field.member_;
            body.extend(quote! {
                st.encode_field(#field_name, &value.#member)
                    .may_cancel_with(cancel.child_token())
                    .await?;
            });
        }
    }
    body.extend(quote! {
        st.end().may_cancel_with(cancel).await
    });
    body
}

fn encode_tuple_body_(name: &str, fields: &[FieldModel]) -> TokenStream {
    let len = fields
        .iter()
        .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
        .count();
    let mut body = quote! {
        let mut st = encoder
            .encode_tuple_struct(#name, #len)
            .may_cancel_with(cancel.child_token())
            .await?;
    };
    for field in fields {
        if field.skip_encoding_ || field.is_phantom_ {
            continue;
        }
        let member = &field.member_;
        body.extend(quote! {
            st.encode_field(&value.#member)
                .may_cancel_with(cancel.child_token())
                .await?;
        });
    }
    body.extend(quote! {
        st.end().may_cancel_with(cancel).await
    });
    body
}

fn encode_enum_body_(
    type_name: &str,
    enum_ident: &syn::Ident,
    variants: &[VariantModel],
) -> TokenStream {
    let mut arms = Vec::with_capacity(variants.len());
    for variant in variants {
        let vident = &variant.ident_;
        let vname = &variant.name_;
        let index = variant.index_;
        let arm = match &variant.fields_ {
            FieldsModel::Unit => quote! {
                #enum_ident::#vident => encoder
                    .encode_unit_variant(#type_name, #index, #vname)
                    .may_cancel_with(cancel)
                    .await
            },
            FieldsModel::Unnamed(fields) if fields.len() == 1 => {
                let binding = &fields[0].binding_;
                quote! {
                    #enum_ident::#vident(#binding) => encoder
                        .encode_newtype_variant(#type_name, #index, #vname, #binding)
                        .may_cancel_with(cancel)
                        .await
                }
            }
            FieldsModel::Unnamed(fields) => {
                let bindings: Vec<_> = fields.iter().map(|field| &field.binding_).collect();
                let len = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .count();
                let encodes = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .map(|field| {
                        let binding = &field.binding_;
                        quote! {
                            st.encode_field(#binding)
                                .may_cancel_with(cancel.child_token())
                                .await?;
                        }
                    });
                quote! {
                    #enum_ident::#vident(#(#bindings),*) => {
                        let mut st = encoder
                            .encode_tuple_variant(#type_name, #index, #vname, #len)
                            .may_cancel_with(cancel.child_token())
                            .await?;
                        #(#encodes)*
                        st.end().may_cancel_with(cancel).await
                    }
                }
            }
            FieldsModel::Named(fields) => {
                let bound: Vec<_> = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .filter_map(|field| field.ident_.as_ref())
                    .collect();
                let has_skipped = fields
                    .iter()
                    .any(|field| field.skip_encoding_ || field.is_phantom_);
                let pattern = if has_skipped {
                    quote! { #enum_ident::#vident { #(#bound,)* .. } }
                } else {
                    quote! { #enum_ident::#vident { #(#bound,)* } }
                };
                let len = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .count();
                let encodes = fields.iter().map(|field| {
                    let field_name = &field.name_;
                    if field.skip_encoding_ || field.is_phantom_ {
                        quote! {
                            st.skip_field(#field_name)
                                .may_cancel_with(cancel.child_token())
                                .await?;
                        }
                    } else {
                        let binding = field.ident_.as_ref().expect("named field");
                        quote! {
                            st.encode_field(#field_name, #binding)
                                .may_cancel_with(cancel.child_token())
                                .await?;
                        }
                    }
                });
                quote! {
                    #pattern => {
                        let mut st = encoder
                            .encode_struct_variant(#type_name, #index, #vname, #len)
                            .may_cancel_with(cancel.child_token())
                            .await?;
                        #(#encodes)*
                        st.end().may_cancel_with(cancel).await
                    }
                }
            }
        };
        arms.push(arm);
    }
    quote! {
        match value {
            #(#arms),*
        }
    }
}

