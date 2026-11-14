//! `TrHasTypeId` 的代码生成。
//!
//! 生成逻辑只依赖 [`TypeModel`]：把字段/变体按 `dev-notes` 的规范语法展开成
//! 隐藏 `Hasher32` / `Hasher64` / `Hasher128` 的调用序列。哈希算法完全由
//! `abs_denco` 的 feature 决定。

use zyn::{
    Span, TokenStream,
    quote::quote,
    syn::{Ident, Type, parse_quote},
};

use crate::model_::{FieldModel, FieldsModel, Kind, TypeModel, VariantModel};

/// 生成 `impl TrHasTypeId`。
pub(crate) fn expand_(model: &TypeModel) -> TokenStream {
    if let Option::Some(inner) = model.transparent_inner_ty_() {
        return expand_transparent_(model, inner);
    }
    expand_structural_(model)
}

/// 生成 transparent newtype 的 impl：三个宽度都直接等于内层类型。
fn expand_transparent_(model: &TypeModel, inner: &Type) -> TokenStream {
    let ident = &model.ident_;
    let ty_generics = model.ty_generics_();
    let mut generics = model.generics_.clone();
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#inner: abs_denco::type_id::TrHasTypeId));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let type_name = type_name_tokens_(ident);
    quote! {
        impl #impl_generics abs_denco::type_id::TrHasTypeId for #ident #ty_generics #where_clause {
            const TYPE_ID_128: u128 = <#inner as abs_denco::type_id::TrHasTypeId>::TYPE_ID_128;
            const TYPE_ID_64: u64 = <#inner as abs_denco::type_id::TrHasTypeId>::TYPE_ID_64;
            const TYPE_ID_32: u32 = <#inner as abs_denco::type_id::TrHasTypeId>::TYPE_ID_32;
            const TYPE_NAME: &'static str = #type_name;
        }
    }
}

/// 生成结构化哈希的 impl。
fn expand_structural_(model: &TypeModel) -> TokenStream {
    let ident = &model.ident_;
    let ty_generics = model.ty_generics_();
    let mut generics = model.generics_.clone();
    let mut bounds = Vec::new();
    collect_bounds_(model, &mut bounds);
    for ty in bounds {
        generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: abs_denco::type_id::TrHasTypeId));
    }
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let segments = segments_(model);
    let body_128 = hash_body_(&segments, Width::U128);
    let body_64 = hash_body_(&segments, Width::U64);
    let body_32 = hash_body_(&segments, Width::U32);
    let type_name = type_name_tokens_(ident);
    quote! {
        impl #impl_generics abs_denco::type_id::TrHasTypeId for #ident #ty_generics #where_clause {
            const TYPE_ID_128: u128 = { #body_128 };
            const TYPE_ID_64: u64 = { #body_64 };
            const TYPE_ID_32: u32 = { #body_32 };
            const TYPE_NAME: &'static str = #type_name;
        }
    }
}

/// 收集需要 `TrHasTypeId` 约束的字段类型。
fn collect_bounds_(model: &TypeModel, out: &mut Vec<Type>) {
    if let Option::Some(inner) = model.transparent_inner_ty_() {
        out.push(inner.clone());
        return;
    }
    match &model.kind_ {
        Kind::Struct(fields) => collect_field_bounds_(fields, out),
        Kind::Enum(variants) => {
            for variant in variants {
                collect_field_bounds_(&variant.fields_, out);
            }
        }
    }
}

/// 收集一个字段列表里参与线上的字段类型。
fn collect_field_bounds_(fields: &FieldsModel, out: &mut Vec<Type>) {
    if let FieldsModel::Named(items) | FieldsModel::Unnamed(items) = fields {
        for field in items {
            if participates_(field) {
                out.push(field.ty_.clone());
            }
        }
    }
}

/// 字段是否出现在规范线上 schema 中。
fn participates_(field: &FieldModel) -> bool {
    !field.is_phantom_ && !(field.skip_encoding_ && field.skip_decoding_)
}

/// 字段方向位；完全跳过的字段返回 `None`。
fn dir_(field: &FieldModel) -> Option<&'static str> {
    if !participates_(field) {
        return Option::None;
    }
    Option::Some(match (field.skip_encoding_, field.skip_decoding_) {
        (false, false) => "DIR_BOTH",
        (true, false) => "DIR_DECODE_ONLY",
        (false, true) => "DIR_ENCODE_ONLY",
        (true, true) => unreachable!("完全跳过的字段已在 participates_ 中排除"),
    })
}

/// 规范输入的一个片段。
enum Segment {
    /// schema 标签/方向位/变体形状，引用 `abs_denco` 的隐藏常量。
    Tag(&'static str),
    /// 字段数/变体数。
    U32(u32),
    /// `::core::module_path!()`。
    ModulePath,
    /// 长度前缀字符串。
    Str(String),
    /// 字段类型的同宽度 ID。
    Id(Type),
}

/// 三个目标宽度。
#[derive(Clone, Copy)]
enum Width {
    U32,
    U64,
    U128,
}

impl Width {
    /// 该宽度使用的隐藏 hasher 类型名。
    fn hasher_(self) -> Ident {
        let name = match self {
            Width::U32 => "Hasher32",
            Width::U64 => "Hasher64",
            Width::U128 => "Hasher128",
        };
        Ident::new(name, Span::call_site())
    }

    /// 该宽度使用的 `TrHasTypeId` 常量名。
    fn id_const_(self) -> Ident {
        let name = match self {
            Width::U32 => "TYPE_ID_32",
            Width::U64 => "TYPE_ID_64",
            Width::U128 => "TYPE_ID_128",
        };
        Ident::new(name, Span::call_site())
    }

    /// 字段 ID 段落的字节数。
    fn id_bytes_(self) -> usize {
        match self {
            Width::U32 => 4,
            Width::U64 => 8,
            Width::U128 => 16,
        }
    }
}

/// 把模型展开为规范片段序列。
fn segments_(model: &TypeModel) -> Vec<Segment> {
    match &model.kind_ {
        Kind::Struct(FieldsModel::Unit) => {
            let mut out = vec![Segment::Tag("STRUCT_UNIT")];
            push_path_(model, &mut out);
            out
        }
        Kind::Struct(FieldsModel::Unnamed(fields)) if fields.len() == 1 => {
            if let Option::Some(inner) = model.transparent_inner_ty_() {
                return vec![Segment::Id(inner.clone())];
            }
            let field = &fields[0];
            let mut out = vec![Segment::Tag("STRUCT_NEWTYPE")];
            push_path_(model, &mut out);
            if let Option::Some(dir) = dir_(field) {
                out.push(Segment::Tag(dir));
            }
            out.push(Segment::Id(field.ty_.clone()));
            out
        }
        Kind::Struct(FieldsModel::Unnamed(fields)) => {
            let mut out = vec![Segment::Tag("STRUCT_TUPLE")];
            push_path_(model, &mut out);
            let fields = participating_fields_(fields);
            out.push(Segment::U32(fields.len() as u32));
            for field in fields {
                if let Option::Some(dir) = dir_(field) {
                    out.push(Segment::Tag(dir));
                }
                out.push(Segment::Id(field.ty_.clone()));
            }
            out
        }
        Kind::Struct(FieldsModel::Named(fields)) => {
            let mut out = vec![Segment::Tag("STRUCT_NAMED")];
            push_path_(model, &mut out);
            let fields = participating_fields_(fields);
            out.push(Segment::U32(fields.len() as u32));
            for field in fields {
                out.push(Segment::Str(field.name_.clone()));
                if let Option::Some(dir) = dir_(field) {
                    out.push(Segment::Tag(dir));
                }
                out.push(Segment::Id(field.ty_.clone()));
            }
            out
        }
        Kind::Enum(variants) => {
            let mut out = vec![Segment::Tag("ENUM")];
            push_path_(model, &mut out);
            let mut variants: Vec<&VariantModel> = variants.iter().collect();
            variants.sort_by(|left, right| left.name_.cmp(&right.name_));
            out.push(Segment::U32(variants.len() as u32));
            for variant in variants {
                out.push(Segment::Str(variant.name_.clone()));
                match &variant.fields_ {
                    FieldsModel::Unit => out.push(Segment::Tag("VARIANT_UNIT")),
                    FieldsModel::Unnamed(fields) if fields.len() == 1 => {
                        out.push(Segment::Tag("VARIANT_NEWTYPE"));
                        let field = &fields[0];
                        if let Option::Some(dir) = dir_(field) {
                            out.push(Segment::Tag(dir));
                        }
                        out.push(Segment::Id(field.ty_.clone()));
                    }
                    FieldsModel::Unnamed(fields) => {
                        out.push(Segment::Tag("VARIANT_TUPLE"));
                        let fields = participating_fields_(fields);
                        out.push(Segment::U32(fields.len() as u32));
                        for field in fields {
                            if let Option::Some(dir) = dir_(field) {
                                out.push(Segment::Tag(dir));
                            }
                            out.push(Segment::Id(field.ty_.clone()));
                        }
                    }
                    FieldsModel::Named(fields) => {
                        out.push(Segment::Tag("VARIANT_NAMED"));
                        let fields = participating_fields_(fields);
                        out.push(Segment::U32(fields.len() as u32));
                        for field in fields {
                            out.push(Segment::Str(field.name_.clone()));
                            if let Option::Some(dir) = dir_(field) {
                                out.push(Segment::Tag(dir));
                            }
                            out.push(Segment::Id(field.ty_.clone()));
                        }
                    }
                }
            }
            out
        }
    }
}

/// 返回参与线上的字段引用。
fn participating_fields_(fields: &[FieldModel]) -> Vec<&FieldModel> {
    fields.iter().filter(|field| participates_(field)).collect()
}

/// 写入 `module_path!()`、Rust 类型名与线上名。
fn push_path_(model: &TypeModel, out: &mut Vec<Segment>) {
    out.push(Segment::ModulePath);
    out.push(Segment::Str(ident_str_(&model.ident_)));
    out.push(Segment::Str(model.rename_.clone()));
}

/// 生成指定宽度的 `Hasher` 调用序列。
fn hash_body_(segments: &[Segment], width: Width) -> TokenStream {
    let hasher = width.hasher_();
    let id_const = width.id_const_();
    let segment_len = segment_len_expr_(segments, width.id_bytes_());
    let mut body = quote! {
        let hasher = abs_denco::type_id::__private::#hasher::new(#segment_len);
    };
    for segment in segments {
        let step = match segment {
            Segment::Tag(name) => {
                let name = Ident::new(name, Span::call_site());
                quote! {
                    let hasher = hasher.u8(abs_denco::type_id::__private::tags::#name);
                }
            }
            Segment::U32(value) => quote! {
                let hasher = hasher.u32(#value);
            },
            Segment::ModulePath => quote! {
                let hasher = hasher.str(::core::module_path!());
            },
            Segment::Str(value) => quote! {
                let hasher = hasher.str(#value);
            },
            Segment::Id(ty) => quote! {
                let hasher = hasher.id(
                    <#ty as abs_denco::type_id::TrHasTypeId>::#id_const
                );
            },
        };
        body.extend(step);
    }
    body.extend(quote! { hasher.finish() });
    body
}

/// 计算一个宽度下 segment 字节数的 const 表达式（不含 hasher 前缀）。
fn segment_len_expr_(segments: &[Segment], id_bytes: usize) -> TokenStream {
    let mut terms = Vec::new();
    for segment in segments {
        let term = match segment {
            Segment::Tag(_) => quote! { 2usize },
            Segment::U32(_) => quote! { 5usize },
            Segment::ModulePath => {
                quote! { (1usize + 4usize + ::core::module_path!().len()) }
            }
            Segment::Str(value) => {
                let len = 1 + 4 + value.len();
                quote! { #len }
            }
            Segment::Id(_) => {
                let len = 1 + id_bytes;
                quote! { #len }
            }
        };
        terms.push(term);
    }
    quote! { 0usize #( + #terms )* }
}

/// `concat!(module_path!(), "::", Rust 类型名)` 的 token。
fn type_name_tokens_(ident: &Ident) -> TokenStream {
    let name = ident_str_(ident);
    quote! { ::core::concat!(::core::module_path!(), "::", #name) }
}

/// Rust ident 的字符串形式；去掉 raw identifier 前缀。
fn ident_str_(ident: &Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_owned()
}

#[cfg(test)]
mod tests_ {
    use zyn::syn;

    use super::*;

    /// 解析一个派生输入为模型。
    fn model_(input: syn::DeriveInput) -> TypeModel {
        crate::model_::parse_(&input).expect("测试输入必须能解析")
    }

    /// 把片段序列渲染为可比较的字符串。
    fn render_(segments: &[Segment]) -> Vec<String> {
        segments
            .iter()
            .map(|segment| match segment {
                Segment::Tag(name) => format!("tag:{name}"),
                Segment::U32(value) => format!("u32:{value}"),
                Segment::ModulePath => "path".to_owned(),
                Segment::Str(value) => format!("str:{value}"),
                Segment::Id(ty) => format!("id:{}", quote!(#ty)),
            })
            .collect()
    }

    /// 测试枚举变体顺序不影响片段序列（生成前按线上名排序）。
    /// Test that enum variant order does not affect the segment sequence.
    ///
    /// - 手段：解析变体顺序相反、类型名相同的两个枚举，渲染片段序列。
    /// - 判定：两个序列完全相同。
    /// - Means: parse two enums with the same ident and reversed variant order.
    /// - Verdict: the rendered segment sequences are identical.
    #[test]
    fn enum_variant_order_is_ignored_() {
        let left: syn::DeriveInput = syn::parse_quote! {
            enum Ordered {
                A,
                B(u8),
                C { x: u16 },
            }
        };
        let right: syn::DeriveInput = syn::parse_quote! {
            enum Ordered {
                C { x: u16 },
                B(u8),
                A,
            }
        };
        assert_eq!(
            render_(&segments_(&model_(left))),
            render_(&segments_(&model_(right)))
        );
    }

    /// 测试结构体字段顺序参与片段序列。
    /// Test that struct field order takes part in the segment sequence.
    ///
    /// - 手段：解析字段顺序相反的两个结构体。
    /// - 判定：两个序列不同。
    /// - Means: parse two structs with reversed field order.
    /// - Verdict: the two sequences differ.
    #[test]
    fn struct_field_order_matters_() {
        let left: syn::DeriveInput = syn::parse_quote! {
            struct Ordered {
                a: u8,
                b: u16,
            }
        };
        let right: syn::DeriveInput = syn::parse_quote! {
            struct Ordered {
                b: u16,
                a: u8,
            }
        };
        assert_ne!(
            render_(&segments_(&model_(left))),
            render_(&segments_(&model_(right)))
        );
    }

    /// 测试重命名、跳过与方向位进入片段序列。
    /// Test that renames, skips and direction bits appear in the segment sequence.
    ///
    /// - 手段：渲染带 `rename` / `skip` / `skip_encoding` / `skip_decoding` 的结构体。
    /// - 判定：出现重命名后的名字；完全跳过的字段不出现；单向跳过使用对应方向标签。
    /// - Means: render a struct with rename/skip/skip_encoding/skip_decoding.
    /// - Verdict: renamed names appear; the fully skipped field is absent; one-way
    ///   skips use the matching direction tag.
    #[test]
    fn attributes_change_segments_() {
        let input: syn::DeriveInput = syn::parse_quote! {
            struct Attributed {
                #[denco(rename = "x")]
                renamed: u8,
                #[denco(skip)]
                gone: u16,
                #[denco(skip_encoding)]
                decode_only: u32,
                #[denco(skip_decoding)]
                encode_only: u64,
            }
        };
        let rendered = render_(&segments_(&model_(input)));
        assert!(rendered.contains(&"str:x".to_owned()));
        assert!(!rendered.iter().any(|item| item == "str:gone"));
        assert!(rendered.contains(&"tag:DIR_DECODE_ONLY".to_owned()));
        assert!(rendered.contains(&"tag:DIR_ENCODE_ONLY".to_owned()));
    }

    /// 测试 transparent newtype 只写内层 ID。
    /// Test that a transparent newtype writes only the inner ID.
    ///
    /// - 手段：解析带 `#[denco(transparent)]` 的 newtype 并渲染片段。
    /// - 判定：片段序列只含一个 `id:` 片段。
    /// - Means: parse a transparent newtype and render its segments.
    /// - Verdict: the sequence contains exactly one `id:` segment.
    #[test]
    fn transparent_writes_only_inner_id_() {
        let input: syn::DeriveInput = syn::parse_quote! {
            #[denco(transparent)]
            struct Wrapper(Vec<u8>);
        };
        let rendered = render_(&segments_(&model_(input)));
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].starts_with("id:"));
    }

    /// 测试展开结果同时包含三个宽度与三个原生 hasher。
    /// Test that the expansion contains all three widths and three native hashers.
    ///
    /// - 手段：展开一个普通结构体并检查 token 文本。
    /// - 判定：同时出现 `TYPE_ID_128/64/32` 与 `Hasher128/64/32`。
    /// - Means: expand a plain struct and inspect the token text.
    /// - Verdict: all three `TYPE_ID_*` consts and all three hashers appear.
    #[test]
    fn expansion_contains_three_widths_() {
        let input: syn::DeriveInput = syn::parse_quote! {
            struct Sample {
                a: u8,
            }
        };
        let tokens = expand_(&model_(input)).to_string();
        for name in [
            "TYPE_ID_128",
            "TYPE_ID_64",
            "TYPE_ID_32",
            "Hasher128",
            "Hasher64",
            "Hasher32",
        ] {
            assert!(tokens.contains(name), "缺少 {name}");
        }
    }
}
