//! 解析 `#[denco(...)]` 辅助属性。

use zyn::syn::{self, Attribute, LitStr, Token};

/// 类型级属性。
#[derive(Clone, Default)]
pub(crate) struct ContainerAttrs {
    pub(crate) rename: Option<String>,
    pub(crate) rename_all: Option<String>,
    pub(crate) transparent: bool,
}

/// 字段级属性。
#[derive(Clone, Default)]
pub(crate) struct FieldAttrs {
    pub(crate) rename: Option<String>,
    pub(crate) aliases: Vec<String>,
    pub(crate) skip_encoding: bool,
    pub(crate) skip_decoding: bool,
    pub(crate) default: bool,
}

/// 枚举变体级属性。
#[derive(Clone, Default)]
pub(crate) struct VariantAttrs {
    pub(crate) rename: Option<String>,
    pub(crate) aliases: Vec<String>,
}

pub(crate) fn parse_container_attrs_(attrs: &[Attribute]) -> syn::Result<ContainerAttrs> {
    let mut out = ContainerAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("denco") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                out.rename = Some(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("rename_all") {
                out.rename_all = Some(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("transparent") {
                expect_flag_(&meta)?;
                out.transparent = true;
            } else {
                return Err(meta.error("未知的 denco 类型属性"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

pub(crate) fn parse_field_attrs_(attrs: &[Attribute]) -> syn::Result<FieldAttrs> {
    let mut out = FieldAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("denco") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                out.rename = Some(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("alias") {
                out.aliases.push(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("skip") {
                expect_flag_(&meta)?;
                out.skip_encoding = true;
                out.skip_decoding = true;
            } else if meta.path.is_ident("skip_encoding")
                || meta.path.is_ident("skip_serializing")
            {
                expect_flag_(&meta)?;
                out.skip_encoding = true;
            } else if meta.path.is_ident("skip_decoding")
                || meta.path.is_ident("skip_deserializing")
            {
                expect_flag_(&meta)?;
                out.skip_decoding = true;
            } else if meta.path.is_ident("default") {
                if meta.input.peek(Token![=]) {
                    return Err(meta.error("denco 暂不支持自定义 default 函数"));
                }
                out.default = true;
            } else {
                return Err(meta.error("未知的 denco 字段属性"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

pub(crate) fn parse_variant_attrs_(attrs: &[Attribute]) -> syn::Result<VariantAttrs> {
    let mut out = VariantAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("denco") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                out.rename = Some(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("alias") {
                out.aliases.push(lit_str_(&meta)?.value());
            } else if meta.path.is_ident("skip") {
                return Err(meta.error("denco 暂不支持跳过整个枚举变体"));
            } else {
                return Err(meta.error("未知的 denco 变体属性"));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

fn lit_str_(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<LitStr> {
    meta.value()?.parse::<LitStr>()
}

fn expect_flag_(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
    if meta.input.peek(Token![=]) {
        return Err(meta.error("该 denco 属性不接受值"));
    }
    Ok(())
}
