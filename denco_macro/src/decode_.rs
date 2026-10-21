//! `TrDecodable` 的代码生成。

use zyn::{
    TokenStream, format_ident,
    quote::quote,
    syn::{self, Ident, parse_quote},
};

use crate::model_::{FieldModel, FieldsModel, Kind, TypeModel, VariantModel};

pub(crate) fn expand_(model: &TypeModel) -> TokenStream {
    let ident = &model.ident_;
    let type_name = ident.to_string();
    let type_name = type_name.trim_start_matches("r#");
    let prefix = format_ident!("__DencoDecode{type_name}");
    let (body, impls) = decode_body_(model, &prefix);

    let tr_cancellation = format_ident!("__DencoDecTrCancellationToken{type_name}");
    let tr_may_cancel = format_ident!("__DencoDecTrMayCancel{type_name}");
    let tr_decoder = format_ident!("__DencoDecTrDecoder{type_name}");
    let map_access = format_ident!("__DencoDecMapAccess{type_name}");
    let seq_access = format_ident!("__DencoDecSeqAccess{type_name}");
    let enum_access = format_ident!("__DencoDecEnumAccess{type_name}");
    let variant_access = format_ident!("__DencoDecVariantAccess{type_name}");

    quote! {
        #[allow(unused_imports)]
        use abs_cancel::{
            TrCancellationToken as #tr_cancellation,
            TrMayCancel as #tr_may_cancel,
        };
        #[allow(unused_imports)]
        use abs_denco::de::{
            EnumAccess as #enum_access,
            MapAccess as #map_access,
            SeqAccess as #seq_access,
            TrDecoder as #tr_decoder,
            VariantAccess as #variant_access,
        };

        #body
        #impls
    }
}

fn name_(prefix: &Ident, suffix: &str) -> Ident {
    format_ident!("{prefix}{suffix}")
}

type Parts = (TokenStream, TokenStream);

fn decode_body_(model: &TypeModel, prefix: &Ident) -> Parts {
    match &model.kind_ {
        Kind::Struct(fields) => decode_struct_(model, prefix, fields),
        Kind::Enum(variants) => decode_enum_(model, prefix, variants),
    }
}

//-- ---- 结构体 ---------------------------------------------------------------

fn decode_struct_(model: &TypeModel, prefix: &Ident, fields: &FieldsModel) -> Parts {
    match fields {
        FieldsModel::Unit => decode_unit_struct_(model, prefix),
        FieldsModel::Unnamed(fields) if fields.len() == 1 => {
            decode_newtype_struct_(model, prefix, fields)
        }
        FieldsModel::Unnamed(fields) => decode_tuple_struct_(model, prefix, fields),
        FieldsModel::Named(fields) => decode_named_struct_(model, prefix, fields),
    }
}

fn decode_unit_struct_(model: &TypeModel, prefix: &Ident) -> Parts {
    let model_name = &model.rename_;
    let ident = &model.ident_;
    let visitor_ident = name_(prefix, "Visitor");
    let visitor = visitor_def_(model, &visitor_ident);
    let visitor_expr = visitor_expr_(model, &visitor_ident);
    let visitor_impl = visitor_impl_(
        model,
        &visitor_ident,
        quote! {

            fn visit_unit<'a, __DencoError>(
                self,
            ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoError>>
            where
                Self: 'a,
                Self::Value: 'a,
                __DencoError: abs_denco::de::Error + 'a,
            {
                ::core::future::ready(::core::result::Result::Ok(#ident))
            }
        },
    );
    let (entry, entry_impl) = decode_entry_(
        model,
        prefix,
        quote! {
            de.decode_unit_struct(#model_name, #visitor_expr)
                .may_cancel_with(cancel)
                .await
        },
        &visitor_ident,
    );
    (
        quote! {
            #visitor
            #visitor_impl
            #entry
        },
        entry_impl,
    )
}

fn decode_newtype_struct_(model: &TypeModel, prefix: &Ident, fields: &[FieldModel]) -> Parts {
    let model_name = &model.rename_;
    let ident = &model.ident_;
    let inner_ty = &fields[0].ty_;
    let visitor_ident = name_(prefix, "Visitor");

    if model.transparent_ {
        let (entry, entry_impl) = decode_entry_(
            model,
            prefix,
            quote! {
                {
                    let value = <#inner_ty as abs_denco::de::TrDecodable<'de>>::decode_async(de)
                        .may_cancel_with(cancel)
                        .await?;
                    ::core::result::Result::Ok(#ident(value))
                }
            },
            &visitor_ident,
        );
        return (entry, entry_impl);
    }

    let newtype_op = format_ident!("{prefix}NewtypeOp");
    let newtype_fn = format_ident!("{prefix}NewtypeOpFn");
    let newtype_async = format_ident!("{prefix}NewtypeOpAsync");
    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    generics.params.insert(1, parse_quote!('a));
    generics.params.push(parse_quote!(__DencoDecoder));
    generics.params.push(parse_quote!(__DencoCancel));
    add_decode_bounds_(&mut generics, model, quote!(__DencoDecoder));
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let ty_generics = model.ty_generics_();
    let type_args = type_args_(model);
    let const_args = const_args_(model);

    let newtype_fn_def = quote! {
        #[gen_mcf2::gen_may_cancel_future(#newtype_op)]
        async fn #newtype_fn #fn_generics (de: __DencoDecoder, cancel: __DencoCancel) -> ::core::result::Result<#ident #ty_generics, __DencoDecoder::Error>
        #where_clause
        {
            let value = <#inner_ty as abs_denco::de::TrDecodable<'de>>::decode_async(de)
                .may_cancel_with(cancel)
                .await?;
            ::core::result::Result::Ok(#ident(value))
        }
    };

    let visitor = visitor_def_(model, &visitor_ident);
    let visitor_expr = visitor_expr_(model, &visitor_ident);
    let visitor_impl = visitor_impl_(
        model,
        &visitor_ident,
        quote! {

            fn visit_newtype_struct<'a, __DencoDecoder>(
                self,
                de: __DencoDecoder,
            ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoDecoder::Error>>
            where
                'de: 'a,
                Self: 'a,
                Self::Value: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
            {
                #newtype_async::<'de, 'a, 'a, #(#type_args,)* __DencoDecoder, #(#const_args,)*>::new(de)
            }
        },
    );
    let (entry, entry_impl) = decode_entry_(
        model,
        prefix,
        quote! {
            de.decode_newtype_struct(#model_name, #visitor_expr)
                .may_cancel_with(cancel)
                .await
        },
        &visitor_ident,
    );
    (
        quote! {
            #newtype_fn_def
            #visitor
            #visitor_impl
            #entry
        },
        entry_impl,
    )
}

fn decode_tuple_struct_(model: &TypeModel, prefix: &Ident, fields: &[FieldModel]) -> Parts {
    let model_name = &model.rename_;
    let visitor_ident = name_(prefix, "Visitor");
    let (seq_method, seq_items) = seq_helper_(model, None, fields, false, &name_(prefix, "SeqOp"));
    let visitor = visitor_def_(model, &visitor_ident);
    let visitor_impl = visitor_impl_(model, &visitor_ident, seq_method);
    let visitor_expr = visitor_expr_(model, &visitor_ident);
    let len = fields
        .iter()
        .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
        .count();
    let (entry, entry_impl) = decode_entry_(
        model,
        prefix,
        quote! {
            de.decode_tuple_struct(#model_name, #len, #visitor_expr)
                .may_cancel_with(cancel)
                .await
        },
        &visitor_ident,
    );
    (
        quote! {
            #visitor
            #visitor_impl
            #seq_items
            #entry
        },
        entry_impl,
    )
}

fn decode_named_struct_(model: &TypeModel, prefix: &Ident, fields: &[FieldModel]) -> Parts {
    let model_name = &model.rename_;
    let visitor_ident = name_(prefix, "Visitor");
    let field_items = field_key_items_(
        fields,
        &name_(prefix, "Field"),
        &name_(prefix, "FieldSeed"),
        &name_(prefix, "FieldVisitor"),
        &name_(prefix, "FieldOp"),
    );
    let (map_method, map_items) = map_helper_(
        model,
        None,
        fields,
        &name_(prefix, "MapOp"),
        &name_(prefix, "FieldSeed"),
        &name_(prefix, "Field"),
    );
    let (seq_method, seq_items) = seq_helper_(model, None, fields, true, &name_(prefix, "SeqOp"));
    let visitor = visitor_def_(model, &visitor_ident);
    let visitor_impl = visitor_impl_(model, &visitor_ident, quote! { #map_method #seq_method });
    let visitor_expr = visitor_expr_(model, &visitor_ident);
    let field_names: Vec<_> = fields
        .iter()
        .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
        .map(|field| field.name_.as_str())
        .collect();
    let (entry, entry_impl) = decode_entry_(
        model,
        prefix,
        quote! {
            de.decode_struct(#model_name, &[#(#field_names),*], #visitor_expr)
                .may_cancel_with(cancel)
                .await
        },
        &visitor_ident,
    );
    (
        quote! {
            #field_items
            #visitor
            #visitor_impl
            #map_items
            #seq_items
            #entry
        },
        entry_impl,
    )
}

//-- ---- 枚举 ---------------------------------------------------------------

fn decode_enum_(model: &TypeModel, prefix: &Ident, variants: &[VariantModel]) -> Parts {
    let model_name = &model.rename_;
    let enum_ident = &model.ident_;
    let variant_enum = name_(prefix, "Variant");
    let variant_seed = name_(prefix, "VariantSeed");
    let variant_visitor = name_(prefix, "VariantVisitor");
    let variant_op = name_(prefix, "VariantOp");
    let variant_items = variant_key_items_(
        variants,
        &variant_enum,
        &variant_seed,
        &variant_visitor,
        &variant_op,
    );

    let mut extra_items = Vec::new();
    let mut arms = Vec::new();
    for variant in variants {
        let vident = &variant.ident_;
        let vindex = variant.index_;
        let variant_variant = format_ident!("V{}", vindex);
        let vname = &variant.name_;
        match &variant.fields_ {
            FieldsModel::Unit => {
                arms.push(quote! {
                    #variant_enum::#variant_variant => {
                        access
                            .unit_variant()
                            .may_cancel_with(cancel)
                            .await?;
                        ::core::result::Result::Ok(#enum_ident::#vident)
                    }
                });
            }
            FieldsModel::Unnamed(fields) if fields.len() == 1 => {
                let ty = &fields[0].ty_;
                arms.push(quote! {
                    #variant_enum::#variant_variant => {
                        let value = access
                            .newtype_variant_seed(::core::marker::PhantomData::<#ty>)
                            .may_cancel_with(cancel)
                            .await?;
                        ::core::result::Result::Ok(#enum_ident::#vident(value))
                    }
                });
            }
            FieldsModel::Unnamed(fields) => {
                let visitor_ident = format_ident!("{prefix}V{vindex}Visitor");
                let (seq_method, seq_items) = seq_helper_(
                    model,
                    Some(variant),
                    fields,
                    matches!(variant.fields_, FieldsModel::Named(_)),
                    &format_ident!("{prefix}V{vindex}SeqOp"),
                );
                let visitor = visitor_def_(model, &visitor_ident);
                let visitor_impl = visitor_impl_(model, &visitor_ident, seq_method);
                extra_items.push(quote! {
                    #visitor
                    #visitor_impl
                    #seq_items
                });
                let len = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .count();
                let visitor_expr = visitor_expr_(model, &visitor_ident);
                arms.push(quote! {
                    #variant_enum::#variant_variant => access
                        .tuple_variant(#len, #visitor_expr)
                        .may_cancel_with(cancel)
                        .await
                });
            }
            FieldsModel::Named(fields) => {
                let visitor_ident = format_ident!("{prefix}V{vindex}Visitor");
                let field_items = field_key_items_(
                    fields,
                    &format_ident!("{prefix}V{vindex}Field"),
                    &format_ident!("{prefix}V{vindex}FieldSeed"),
                    &format_ident!("{prefix}V{vindex}FieldVisitor"),
                    &format_ident!("{prefix}V{vindex}FieldOp"),
                );
                let (map_method, map_items) = map_helper_(
                    model,
                    Some(variant),
                    fields,
                    &format_ident!("{prefix}V{vindex}MapOp"),
                    &format_ident!("{prefix}V{vindex}FieldSeed"),
                    &format_ident!("{prefix}V{vindex}Field"),
                );
                let (seq_method, seq_items) = seq_helper_(
                    model,
                    Some(variant),
                    fields,
                    matches!(variant.fields_, FieldsModel::Named(_)),
                    &format_ident!("{prefix}V{vindex}SeqOp"),
                );
                let visitor = visitor_def_(model, &visitor_ident);
                let visitor_impl =
                    visitor_impl_(model, &visitor_ident, quote! { #map_method #seq_method });
                extra_items.push(quote! {
                    #field_items
                    #visitor
                    #visitor_impl
                    #map_items
                    #seq_items
                });
                let field_names: Vec<_> = fields
                    .iter()
                    .filter(|field| !field.skip_encoding_ && !field.is_phantom_)
                    .map(|field| field.name_.as_str())
                    .collect();
                let visitor_expr = visitor_expr_(model, &visitor_ident);
                arms.push(quote! {
                    #variant_enum::#variant_variant => access
                        .struct_variant(&[#(#field_names),*], #visitor_expr)
                        .may_cancel_with(cancel)
                        .await
                });
            }
        }
        let _ = vname;
    }

    let dispatch_op = name_(prefix, "VariantsOp");
    let dispatch_fn = name_(prefix, "VariantsOpFn");
    let dispatch_async = name_(prefix, "VariantsOpAsync");
    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    generics.params.insert(1, parse_quote!('a));
    generics.params.push(parse_quote!(__DencoAccess));
    generics.params.push(parse_quote!(__DencoCancel));
    {
        let where_clause = generics.make_where_clause();
        for param in model.type_params_() {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
        }
        where_clause.predicates.push(parse_quote!('de: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess: abs_denco::de::EnumAccess<'de> + 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess::Error: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoCancel: abs_cancel::TrCancellationToken));
    }
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let ty_generics = model.ty_generics_();
    let type_args = type_args_(model);
    let const_args = const_args_(model);
    let dispatch_fn_def = quote! {
        #[gen_mcf2::gen_may_cancel_future(#dispatch_op)]
        async fn #dispatch_fn #fn_generics (
            data: __DencoAccess,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#enum_ident #ty_generics, __DencoAccess::Error>
        #where_clause
        {
            let (variant, access) = data
                .variant_seed(#variant_seed)
                .may_cancel_with(cancel.child_token())
                .await?;
            match variant {
                #(#arms),*
            }
        }
    };

    let visitor_ident = name_(prefix, "EnumVisitor");
    let visitor = visitor_def_(model, &visitor_ident);
    let visitor_expr = visitor_expr_(model, &visitor_ident);
    let visitor_impl = visitor_impl_(
        model,
        &visitor_ident,
        quote! {

            fn visit_enum<'a, __DencoAccess>(
                self,
                data: __DencoAccess,
            ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoAccess::Error>>
            where
                'de: 'a,
                Self: 'a,
                Self::Value: 'a,
                __DencoAccess: abs_denco::de::EnumAccess<'de> + 'a,
                __DencoAccess::Error: 'a,
            {
                #dispatch_async::<'de, 'a, 'a, #(#type_args,)* __DencoAccess, #(#const_args,)*>::new(data)
            }
        },
    );
    let variant_names: Vec<_> = variants
        .iter()
        .map(|variant| variant.name_.as_str())
        .collect();
    let (entry, entry_impl) = decode_entry_(
        model,
        prefix,
        quote! {
            de.decode_enum(#model_name, &[#(#variant_names),*], #visitor_expr)
                .may_cancel_with(cancel)
                .await
        },
        &visitor_ident,
    );

    (
        quote! {
            #variant_items
            #dispatch_fn_def
            #(#extra_items)*
            #visitor
            #visitor_impl
            #entry
        },
        entry_impl,
    )
}

//-- ---- 公共构件 -------------------------------------------------------------

fn decode_entry_(
    model: &TypeModel,
    prefix: &Ident,
    decode_call: TokenStream,
    _visitor_ident: &Ident,
) -> Parts {
    let ident = &model.ident_;
    let ty_generics = model.ty_generics_();
    let type_params = model.type_params_();
    let type_args = type_args_(model);
    let const_args = const_args_(model);
    let op_ident = name_(prefix, "Op");
    let op_fn = name_(prefix, "OpFn");
    let op_async = name_(prefix, "OpAsync");

    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    generics.params.insert(1, parse_quote!('a));
    generics.params.push(parse_quote!(__DencoDecoder));
    generics.params.push(parse_quote!(__DencoCancel));
    add_decode_bounds_(&mut generics, model, quote!(__DencoDecoder));
    let (fn_generics, _, where_clause) = generics.split_for_impl();

    let vis = &model.vis_;
    let helper = quote! {
        #[gen_mcf2::gen_may_cancel_future(#op_ident)]
        #[doc(hidden)]
        #[allow(missing_docs)]
        #vis async fn #op_fn #fn_generics (
            de: __DencoDecoder,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#ident #ty_generics, __DencoDecoder::Error>
        #where_clause
        {
            #decode_call
        }
    };

    let mut impl_generics = model.generics_.clone();
    impl_generics.params.insert(0, parse_quote!('de));
    {
        let where_clause = impl_generics.make_where_clause();
        match model.transparent_inner_ty_() {
            Option::Some(inner_ty) => where_clause
                .predicates
                .push(parse_quote!(#inner_ty: abs_denco::de::TrDecodable<'de> + 'de)),
            Option::None => {
                for param in &type_params {
                    where_clause
                        .predicates
                        .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
                }
            }
        }
    }
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
    let impl_ = quote! {
        impl #impl_generics abs_denco::de::TrDecodable<'de> for #ident #ty_generics #where_clause {
            type DecodeAsync<'a, __DencoDecoder>
                = #op_async<'de, 'a, 'a, #(#type_args,)* __DencoDecoder, #(#const_args,)*>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a;

            fn decode_async<'a, __DencoDecoder>(de: __DencoDecoder) -> Self::DecodeAsync<'a, __DencoDecoder>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a,
            {
                #op_async::new(de)
            }
        }
    };
    (helper, impl_)
}

fn add_decode_bounds_(generics: &mut syn::Generics, model: &TypeModel, decoder: TokenStream) {
    let where_clause = generics.make_where_clause();
    where_clause.predicates.push(parse_quote!('de: 'a));
    match model.transparent_inner_ty_() {
        Option::Some(inner_ty) => where_clause
            .predicates
            .push(parse_quote!(#inner_ty: abs_denco::de::TrDecodable<'de> + 'de)),
        Option::None => {
            for param in model.type_params_() {
                where_clause
                    .predicates
                    .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
            }
        }
    }
    where_clause
        .predicates
        .push(parse_quote!(#decoder: abs_denco::de::TrDecoder<'de> + 'a));
    where_clause
        .predicates
        .push(parse_quote!(#decoder::Error: 'a));
    where_clause
        .predicates
        .push(parse_quote!(__DencoCancel: abs_cancel::TrCancellationToken));
}

fn visitor_def_(model: &TypeModel, visitor_ident: &Ident) -> TokenStream {
    let generics = model.generics_.clone();
    let type_params = model.type_params_();
    let (struct_generics, _, where_clause) = generics.split_for_impl();
    quote! {
        struct #visitor_ident #struct_generics (
            ::core::marker::PhantomData<(#(#type_params,)*)>
        ) #where_clause;
    }
}

fn visitor_expr_(model: &TypeModel, visitor_ident: &Ident) -> TokenStream {
    let args = visitor_args_(model);
    if args.is_empty() {
        quote! { #visitor_ident(::core::marker::PhantomData) }
    } else {
        quote! { #visitor_ident::<#(#args),*>(::core::marker::PhantomData) }
    }
}

fn visitor_impl_(model: &TypeModel, visitor_ident: &Ident, methods: TokenStream) -> TokenStream {
    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    {
        let where_clause = generics.make_where_clause();
        for param in model.type_params_() {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
        }
    }
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let ty_generics = model.ty_generics_();
    let ident = &model.ident_;
    let expecting = &model.rename_;
    quote! {
        impl #impl_generics abs_denco::de::Visitor<'de> for #visitor_ident #ty_generics #where_clause {
            type Value = #ident #ty_generics;

            fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(#expecting)
            }

            #methods
        }
    }
}

fn field_key_items_(
    fields: &[FieldModel],
    enum_ident: &Ident,
    seed_ident: &Ident,
    visitor_ident: &Ident,
    op_ident: &Ident,
) -> TokenStream {
    let op_async = format_ident!("{}Async", op_ident);
    let field_op_fn = format_ident!("{}_fn", op_ident.to_string().to_lowercase());
    let has_ignore = fields
        .iter()
        .any(|field| field.skip_decoding_ || field.is_phantom_);
    let mut variants = Vec::new();
    let mut arms = Vec::new();
    let mut expected = Vec::new();

    for (index, field) in fields.iter().enumerate() {
        if field.skip_decoding_ || field.is_phantom_ {
            for name in core::iter::once(&field.name_).chain(field.aliases_.iter()) {
                arms.push(quote! { #name => ::core::future::ready(::core::result::Result::Ok(#enum_ident::Ignore)) });
            }
        } else {
            expected.push(field.name_.as_str());
            let variant = format_ident!("F{index}");
            variants.push(quote! { #variant });
            for name in core::iter::once(&field.name_).chain(field.aliases_.iter()) {
                arms.push(quote! { #name => ::core::future::ready(::core::result::Result::Ok(#enum_ident::#variant)) });
            }
        }
    }
    if has_ignore {
        variants.push(quote! { Ignore });
    }

    let enum_def = if variants.is_empty() {
        quote! { enum #enum_ident {} }
    } else {
        quote! { enum #enum_ident { #(#variants),* } }
    };

    quote! {
        #enum_def

        struct #visitor_ident;

        impl<'de> abs_denco::de::Visitor<'de> for #visitor_ident {
            type Value = #enum_ident;

            fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str("a field identifier")
            }

            fn visit_str<'a, __DencoError>(
                self,
                value: &'a str,
            ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoError>>
            where
                Self: 'a,
                Self::Value: 'a,
                __DencoError: abs_denco::de::Error + 'a,
            {
                match value {
                    #(#arms,)*
                    _ => ::core::future::ready(::core::result::Result::Err(
                        <__DencoError as abs_denco::de::Error>::unknown_field(
                            value,
                            &[#(#expected),*],
                        ),
                    )),
                }
            }
        }

        struct #seed_ident;

        impl<'de> abs_denco::de::TrDecodableSeed<'de> for #seed_ident {
            type Value = #enum_ident;

            type DecodeAsync<'a, __DencoDecoder>
                = #op_async<'de, 'a, 'a, __DencoDecoder>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a,
                Self::Value: 'a;

            fn decode_async<'a, __DencoDecoder>(self, de: __DencoDecoder) -> Self::DecodeAsync<'a, __DencoDecoder>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a,
                Self::Value: 'a,
            {
                #op_async::new(de)
            }
        }

        #[gen_mcf2::gen_may_cancel_future(#op_ident)]
        async fn #field_op_fn<'de, 'a, __DencoDecoder, __DencoCancel>(
            de: __DencoDecoder,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#enum_ident, __DencoDecoder::Error>
        where
            'de: 'a,
            __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
            __DencoDecoder::Error: 'a,
            __DencoCancel: abs_cancel::TrCancellationToken,
        {
            de.decode_identifier(#visitor_ident)
                .may_cancel_with(cancel)
                .await
        }
    }
}

fn map_helper_(
    model: &TypeModel,
    variant: Option<&VariantModel>,
    fields: &[FieldModel],
    op_ident: &Ident,
    field_seed: &Ident,
    field_enum: &Ident,
) -> (TokenStream, TokenStream) {
    let map_fn = format_ident!("{}_fn", op_ident.to_string().to_lowercase());
    let map_async = format_ident!("{}Async", op_ident);

    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    generics.params.insert(1, parse_quote!('a));
    generics.params.push(parse_quote!(__DencoAccess));
    generics.params.push(parse_quote!(__DencoCancel));
    {
        let where_clause = generics.make_where_clause();
        for param in model.type_params_() {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
        }
        where_clause.predicates.push(parse_quote!('de: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess: abs_denco::de::MapAccess<'de> + 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess::Error: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoCancel: abs_cancel::TrCancellationToken));
        for field in fields {
            if field.skip_decoding_ || field.is_phantom_ {
                let ty = &field.ty_;
                where_clause
                    .predicates
                    .push(parse_quote!(#ty: ::core::default::Default));
            }
        }
    }
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let ty_generics = model.ty_generics_();
    let target_ty = &model.ident_;
    let type_args = type_args_(model);
    let const_args = const_args_(model);

    let mut init = Vec::new();
    let mut arms = Vec::new();
    let mut finals = Vec::new();
    let mut construct_fields = Vec::new();

    for (index, field) in fields.iter().enumerate() {
        let binding = &field.binding_;
        let field_name = &field.name_;
        let ty = &field.ty_;
        if field.skip_decoding_ || field.is_phantom_ {
            finals.push(quote! {
                let #binding = <#ty as ::core::default::Default>::default();
            });
            if let Some(ident) = &field.ident_ {
                construct_fields.push(quote! { #ident });
            } else {
                construct_fields.push(quote! { #binding });
            }
            continue;
        }
        init.push(quote! {
            let mut #binding: ::core::option::Option<#ty> = ::core::option::Option::None;
        });
        let variant = format_ident!("F{index}");
        arms.push(quote! {
            #field_enum::#variant => {
                if #binding.is_some() {
                    return ::core::result::Result::Err(
                        <__DencoAccess::Error as abs_denco::de::Error>::duplicate_field(#field_name),
                    );
                }
                #binding = ::core::option::Option::Some(
                    map.next_value_seed(::core::marker::PhantomData::<#ty>)
                        .may_cancel_with(cancel.child_token())
                        .await?,
                );
            }
        });
        let finish = if field.is_option_ {
            quote! { #binding.unwrap_or(::core::option::Option::None) }
        } else if field.default_ {
            quote! { #binding.unwrap_or_default() }
        } else {
            quote! {
                #binding.ok_or_else(|| {
                    <__DencoAccess::Error as abs_denco::de::Error>::missing_field(#field_name)
                })?
            }
        };
        finals.push(quote! { let #binding = #finish; });
        if let Some(ident) = &field.ident_ {
            construct_fields.push(quote! { #ident });
        } else {
            construct_fields.push(quote! { #binding });
        }
    }

    let has_ignore = fields
        .iter()
        .any(|field| field.skip_decoding_ || field.is_phantom_);
    let ignore_arm = if has_ignore {
        quote! {
            #field_enum::Ignore => {
                let _: abs_denco::de::IgnoredAny = map
                    .next_value_seed(::core::marker::PhantomData::<abs_denco::de::IgnoredAny>)
                    .may_cancel_with(cancel.child_token())
                    .await?;
            }
        }
    } else {
        quote! {}
    };
    // 字段 key 枚举的模块前缀与 variant 相关；这里通过 seed 名推导。

    let construct = match variant {
        Some(variant) => {
            let vident = &variant.ident_;
            quote! { #target_ty::#vident { #(#construct_fields),* } }
        }
        None => quote! { #target_ty { #(#construct_fields),* } },
    };

    let helper = quote! {
        #[gen_mcf2::gen_may_cancel_future(#op_ident)]
        async fn #map_fn #fn_generics (
            mut map: __DencoAccess,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#target_ty #ty_generics, __DencoAccess::Error>
        #where_clause
        {
            #(#init)*
            while let ::core::option::Option::Some(key) = map
                .next_key_seed(#field_seed)
                .may_cancel_with(cancel.child_token())
                .await?
            {
                match key {
                    #(#arms)*
                    #ignore_arm
                }
            }
            #(#finals)*
            ::core::result::Result::Ok(#construct)
        }
    };

    let visitor_method = quote! {
        fn visit_map<'a, __DencoAccess>(
            self,
            map: __DencoAccess,
        ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoAccess::Error>>
        where
            'de: 'a,
            Self: 'a,
            Self::Value: 'a,
            __DencoAccess: abs_denco::de::MapAccess<'de> + 'a,
            __DencoAccess::Error: 'a,
        {
            #map_async::<'de, 'a, 'a, #(#type_args,)* __DencoAccess, #(#const_args,)*>::new(map)
        }
    };

    (visitor_method, helper)
}

fn seq_helper_(
    model: &TypeModel,
    variant: Option<&VariantModel>,
    fields: &[FieldModel],
    named: bool,
    op_ident: &Ident,
) -> (TokenStream, TokenStream) {
    let seq_fn = format_ident!("{}_fn", op_ident.to_string().to_lowercase());
    let seq_async = format_ident!("{}Async", op_ident);
    let mut generics = model.generics_.clone();
    generics.params.insert(0, parse_quote!('de));
    generics.params.insert(1, parse_quote!('a));
    generics.params.push(parse_quote!(__DencoAccess));
    generics.params.push(parse_quote!(__DencoCancel));
    {
        let where_clause = generics.make_where_clause();
        for param in model.type_params_() {
            where_clause
                .predicates
                .push(parse_quote!(#param: abs_denco::de::TrDecodable<'de> + 'de));
        }
        where_clause.predicates.push(parse_quote!('de: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess: abs_denco::de::SeqAccess<'de> + 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoAccess::Error: 'a));
        where_clause
            .predicates
            .push(parse_quote!(__DencoCancel: abs_cancel::TrCancellationToken));
        for field in fields {
            if field.skip_decoding_ || field.is_phantom_ {
                let ty = &field.ty_;
                where_clause
                    .predicates
                    .push(parse_quote!(#ty: ::core::default::Default));
            }
        }
    }
    let (fn_generics, _, where_clause) = generics.split_for_impl();
    let ty_generics = model.ty_generics_();
    let target_ty = &model.ident_;
    let type_args = type_args_(model);
    let const_args = const_args_(model);

    let mut reads = Vec::new();
    let mut construct_fields = Vec::new();
    let mut seq_index = 0usize;
    let expecting = match variant {
        Some(variant) => format!(
            "struct variant {}::{} with {} fields",
            target_ty,
            variant.ident_,
            fields
                .iter()
                .filter(|f| !f.skip_decoding_ && !f.is_phantom_)
                .count()
        ),
        None => format!(
            "struct {} with {} fields",
            target_ty,
            fields
                .iter()
                .filter(|f| !f.skip_decoding_ && !f.is_phantom_)
                .count()
        ),
    };
    for field in fields {
        let binding = &field.binding_;
        let ty = &field.ty_;
        if field.skip_decoding_ || field.is_phantom_ {
            reads.push(quote! {
                let #binding = <#ty as ::core::default::Default>::default();
            });
        } else {
            let index = seq_index;
            seq_index += 1;
            reads.push(quote! {
                let #binding = seq
                    .next_element_seed(::core::marker::PhantomData::<#ty>)
                    .may_cancel_with(cancel.child_token())
                    .await?
                    .ok_or_else(|| {
                        <__DencoAccess::Error as abs_denco::de::Error>::invalid_length(#index, &#expecting)
                    })?;
            });
        }
        if let Some(ident) = &field.ident_ {
            construct_fields.push(quote! { #ident });
        } else {
            construct_fields.push(quote! { #binding });
        }
    }
    let construct = match variant {
        Some(variant) => {
            let vident = &variant.ident_;
            if named {
                quote! { #target_ty::#vident { #(#construct_fields),* } }
            } else {
                quote! { #target_ty::#vident(#(#construct_fields),*) }
            }
        }
        None => {
            if named {
                quote! { #target_ty { #(#construct_fields),* } }
            } else {
                quote! { #target_ty(#(#construct_fields),*) }
            }
        }
    };

    let helper = quote! {
        #[gen_mcf2::gen_may_cancel_future(#op_ident)]
        async fn #seq_fn #fn_generics (
            mut seq: __DencoAccess,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#target_ty #ty_generics, __DencoAccess::Error>
        #where_clause
        {
            #(#reads)*
            ::core::result::Result::Ok(#construct)
        }
    };

    let visitor_method = quote! {
        fn visit_seq<'a, __DencoAccess>(
            self,
            seq: __DencoAccess,
        ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoAccess::Error>>
        where
            'de: 'a,
            Self: 'a,
            Self::Value: 'a,
            __DencoAccess: abs_denco::de::SeqAccess<'de> + 'a,
            __DencoAccess::Error: 'a,
        {
            #seq_async::<'de, 'a, 'a, #(#type_args,)* __DencoAccess, #(#const_args,)*>::new(seq)
        }
    };

    (visitor_method, helper)
}

fn variant_key_items_(
    variants: &[VariantModel],
    enum_ident: &Ident,
    seed_ident: &Ident,
    visitor_ident: &Ident,
    op_ident: &Ident,
) -> TokenStream {
    let op_async = format_ident!("{}Async", op_ident);
    let variant_op_fn = format_ident!("{}_fn", op_ident.to_string().to_lowercase());
    let mut variant_defs = Vec::new();
    let mut arms = Vec::new();
    let mut expected = Vec::new();
    for variant in variants {
        let vident = format_ident!("V{}", variant.index_);
        variant_defs.push(quote! { #vident });
        expected.push(variant.name_.as_str());
        for name in core::iter::once(&variant.name_).chain(variant.aliases_.iter()) {
            arms.push(quote! { #name => ::core::future::ready(::core::result::Result::Ok(#enum_ident::#vident)) });
        }
    }
    quote! {
        enum #enum_ident { #(#variant_defs),* }

        struct #visitor_ident;

        impl<'de> abs_denco::de::Visitor<'de> for #visitor_ident {
            type Value = #enum_ident;

            fn expecting(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str("a variant identifier")
            }

            fn visit_str<'a, __DencoError>(
                self,
                value: &'a str,
            ) -> impl abs_cancel::TrMayCancel<'a, MayCancelOutput = ::core::result::Result<Self::Value, __DencoError>>
            where
                Self: 'a,
                Self::Value: 'a,
                __DencoError: abs_denco::de::Error + 'a,
            {
                match value {
                    #(#arms,)*
                    _ => ::core::future::ready(::core::result::Result::Err(
                        <__DencoError as abs_denco::de::Error>::unknown_variant(
                            value,
                            &[#(#expected),*],
                        ),
                    )),
                }
            }
        }

        struct #seed_ident;

        impl<'de> abs_denco::de::TrDecodableSeed<'de> for #seed_ident {
            type Value = #enum_ident;

            type DecodeAsync<'a, __DencoDecoder>
                = #op_async<'de, 'a, 'a, __DencoDecoder>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a,
                Self::Value: 'a;

            fn decode_async<'a, __DencoDecoder>(self, de: __DencoDecoder) -> Self::DecodeAsync<'a, __DencoDecoder>
            where
                'de: 'a,
                __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
                __DencoDecoder::Error: 'a,
                Self: 'a,
                Self::Value: 'a,
            {
                #op_async::new(de)
            }
        }

        #[gen_mcf2::gen_may_cancel_future(#op_ident)]
        async fn #variant_op_fn<'de, 'a, __DencoDecoder, __DencoCancel>(
            de: __DencoDecoder,
            cancel: __DencoCancel,
        ) -> ::core::result::Result<#enum_ident, __DencoDecoder::Error>
        where
            'de: 'a,
            __DencoDecoder: abs_denco::de::TrDecoder<'de> + 'a,
            __DencoDecoder::Error: 'a,
            __DencoCancel: abs_cancel::TrCancellationToken,
        {
            de.decode_identifier(#visitor_ident)
                .may_cancel_with(cancel)
                .await
        }
    }
}

fn type_args_(model: &TypeModel) -> Vec<Ident> {
    model
        .generics_
        .type_params()
        .map(|param| param.ident.clone())
        .collect()
}

fn const_args_(model: &TypeModel) -> Vec<Ident> {
    model
        .generics_
        .const_params()
        .map(|param| param.ident.clone())
        .collect()
}

fn visitor_args_(model: &TypeModel) -> Vec<Ident> {
    model
        .generics_
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) => Some(param.ident.clone()),
            syn::GenericParam::Const(param) => Some(param.ident.clone()),
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect()
}
