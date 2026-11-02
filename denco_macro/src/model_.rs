//! derive 输入的数据模型。

use zyn::{
    Span, format_ident,
    syn::{
        self, Data, DeriveInput, Fields, GenericParam, Ident, Index, Member, Type, fold::Fold,
        spanned::Spanned,
    },
};

use crate::{
    attrs_::{self, ContainerAttrs, FieldAttrs, VariantAttrs},
    names_,
};

#[derive(Clone)]
pub(crate) struct TypeModel {
    pub(crate) ident_: Ident,
    pub(crate) vis_: syn::Visibility,
    pub(crate) generics_: syn::Generics,
    pub(crate) rename_: String,
    pub(crate) transparent_: bool,
    /// 解码模型是否已把用户生命周期绑定到 `'alloc`（用于生成值类型泛型）。
    pub(crate) lifetime_bound_: bool,
    pub(crate) kind_: Kind,
}

#[derive(Clone)]
pub(crate) enum Kind {
    Struct(FieldsModel),
    Enum(Vec<VariantModel>),
}

#[derive(Clone)]
pub(crate) enum FieldsModel {
    Unit,
    Named(Vec<FieldModel>),
    Unnamed(Vec<FieldModel>),
}

#[derive(Clone)]
pub(crate) struct FieldModel {
    pub(crate) ident_: Option<Ident>,
    pub(crate) member_: Member,
    pub(crate) binding_: Ident,
    pub(crate) ty_: Type,
    pub(crate) name_: String,
    pub(crate) aliases_: Vec<String>,
    pub(crate) skip_encoding_: bool,
    pub(crate) skip_decoding_: bool,
    pub(crate) default_: bool,
    pub(crate) is_option_: bool,
    pub(crate) is_phantom_: bool,
}

#[derive(Clone)]
pub(crate) struct VariantModel {
    pub(crate) ident_: Ident,
    pub(crate) name_: String,
    pub(crate) aliases_: Vec<String>,
    pub(crate) index_: u32,
    pub(crate) fields_: FieldsModel,
}

impl TypeModel {
    pub(crate) fn type_params_(&self) -> Vec<Ident> {
        self.generics_
            .type_params()
            .map(|param| param.ident.clone())
            .collect()
    }

    pub(crate) fn ty_generics_(&self) -> syn::TypeGenerics<'_> {
        let (_, ty_generics, _) = self.generics_.split_for_impl();
        ty_generics
    }

    /// 若本类型是 `#[denco(transparent)]` 的 newtype，返回其内层字段类型。
    ///
    /// 透明 newtype 的编解码约束应落在内层类型上，而不是落在类型参数上：
    /// 生成器据此区分「要求 `U16OrStr<S>: TrEncodable`」与「要求 `S: TrEncodable`」。
    ///
    /// Returns the inner field type of a `#[denco(transparent)]` newtype, if any.
    /// The generated impls put their codec bounds on this inner type instead of on the
    /// type parameters, so a newtype can forward to a hand-written inner impl.
    pub(crate) fn transparent_inner_ty_(&self) -> Option<&Type> {
        if !self.transparent_ {
            return Option::None;
        }
        match &self.kind_ {
            Kind::Struct(FieldsModel::Unnamed(fields)) if fields.len() == 1 => {
                Option::Some(&fields[0].ty_)
            }
            _ => Option::None,
        }
    }
}

pub(crate) fn parse_(input: &DeriveInput) -> syn::Result<TypeModel> {
    let attrs = attrs_::parse_container_attrs_(&input.attrs)?;

    let rename = attrs
        .rename
        .clone()
        .unwrap_or_else(|| input.ident.to_string());
    let kind = parse_kind_(input, &attrs)?;

    if attrs.transparent
        && !matches!(&kind, Kind::Struct(FieldsModel::Unnamed(fields)) if fields.len() == 1)
    {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[denco(transparent)] 只能用于恰好一个字段的 newtype 结构体",
        ));
    }

    Ok(TypeModel {
        ident_: input.ident.clone(),
        vis_: input.vis.clone(),
        generics_: input.generics.clone(),
        rename_: rename,
        transparent_: attrs.transparent,
        lifetime_bound_: false,
        kind_: kind,
    })
}

/// 生成解码专用的模型：把所有用户生命周期参数绑定到 `'alloc`。
///
/// 解码结果可以借用 decoder 的 arena（`'alloc`），而用户写的 `Foo<'a>` 里的
/// `'a` 在解码侧必须就是这个 arena 寿命。这里把所有用户生命周期替换为
/// `'alloc`，并从泛型参数列表里移除生命周期参数；类型参数与 const 参数原样
/// 保留。编码侧仍使用原始模型（`Foo<'a>` 的 `impl<'a> TrEncodable`）。
pub(crate) fn bound_lifetimes_to_alloc_(model: &TypeModel) -> TypeModel {
    let names: Vec<String> = model
        .generics_
        .lifetimes()
        .map(|param| param.lifetime.ident.to_string())
        .collect();
    if names.is_empty() {
        return model.clone();
    }

    let mut folder = BindToAlloc_ {
        names_: names,
        alloc_: syn::Lifetime::new("'alloc", Span::call_site()),
    };
    let mut out = model.clone();
    match &mut out.kind_ {
        Kind::Struct(fields) => fold_fields_(fields, &mut folder),
        Kind::Enum(variants) => {
            for variant in variants {
                fold_fields_(&mut variant.fields_, &mut folder);
            }
        }
    }
    out.generics_ = folder.fold_generics(out.generics_.clone());
    out.generics_.params = out
        .generics_
        .params
        .iter()
        .filter(|param| !matches!(param, GenericParam::Lifetime(_)))
        .cloned()
        .collect();
    out.lifetime_bound_ = true;
    out
}

/// 判断类型里是否出现指定生命周期（用于给借用型字段补 `TrDecodable` 约束）。
pub(crate) fn ty_mentions_lifetime_(ty: &Type, name: &str) -> bool {
    let mut finder = LifetimeFinder_ {
        name_: name,
        found_: false,
    };
    let _ = finder.fold_type(ty.clone());
    finder.found_
}

struct BindToAlloc_ {
    names_: Vec<String>,
    alloc_: syn::Lifetime,
}

impl Fold for BindToAlloc_ {
    fn fold_lifetime(&mut self, lifetime: syn::Lifetime) -> syn::Lifetime {
        if self
            .names_
            .iter()
            .any(|name| lifetime.ident == name.as_str())
        {
            self.alloc_.clone()
        } else {
            lifetime
        }
    }
}

struct LifetimeFinder_<'a> {
    name_: &'a str,
    found_: bool,
}

impl Fold for LifetimeFinder_<'_> {
    fn fold_lifetime(&mut self, lifetime: syn::Lifetime) -> syn::Lifetime {
        if lifetime.ident == self.name_ {
            self.found_ = true;
        }
        lifetime
    }
}

fn fold_fields_(fields: &mut FieldsModel, folder: &mut impl Fold) {
    match fields {
        FieldsModel::Unit => {}
        FieldsModel::Named(items) | FieldsModel::Unnamed(items) => {
            for field in items {
                field.ty_ = folder.fold_type(field.ty_.clone());
            }
        }
    }
}

fn parse_kind_(input: &DeriveInput, attrs: &ContainerAttrs) -> syn::Result<Kind> {
    match &input.data {
        Data::Struct(data) => Ok(Kind::Struct(parse_fields_(
            &data.fields,
            attrs.rename_all.as_deref(),
        )?)),
        Data::Enum(data) => {
            let mut variants = Vec::with_capacity(data.variants.len());
            for (index, variant) in data.variants.iter().enumerate() {
                let vattrs: VariantAttrs = attrs_::parse_variant_attrs_(&variant.attrs)?;
                let name = match &vattrs.rename {
                    Some(name) => name.clone(),
                    None => apply_rename_all_(attrs, &variant.ident, variant.span())?,
                };
                let fields = parse_fields_(&variant.fields, None)?;
                variants.push(VariantModel {
                    ident_: variant.ident.clone(),
                    name_: name,
                    aliases_: vattrs.aliases,
                    index_: index as u32,
                    fields_: fields,
                });
            }
            Ok(Kind::Enum(variants))
        }
        Data::Union(data) => Err(syn::Error::new_spanned(
            data.union_token,
            "denco derive 不支持 union",
        )),
    }
}

fn parse_fields_(fields: &Fields, rename_all: Option<&str>) -> syn::Result<FieldsModel> {
    match fields {
        Fields::Unit => Ok(FieldsModel::Unit),
        Fields::Named(named) => {
            let mut out = Vec::with_capacity(named.named.len());
            for (index, field) in named.named.iter().enumerate() {
                out.push(parse_field_(field, index, true, rename_all)?);
            }
            Ok(FieldsModel::Named(out))
        }
        Fields::Unnamed(unnamed) => {
            let mut out = Vec::with_capacity(unnamed.unnamed.len());
            for (index, field) in unnamed.unnamed.iter().enumerate() {
                out.push(parse_field_(field, index, false, rename_all)?);
            }
            Ok(FieldsModel::Unnamed(out))
        }
    }
}

fn parse_field_(
    field: &syn::Field,
    index: usize,
    named: bool,
    rename_all: Option<&str>,
) -> syn::Result<FieldModel> {
    let attrs: FieldAttrs = attrs_::parse_field_attrs_(&field.attrs)?;
    let ident = field.ident.clone();
    let base_name = ident
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| index.to_string());
    let base_name = base_name
        .strip_prefix("r#")
        .unwrap_or(&base_name)
        .to_owned();
    let name = match &attrs.rename {
        Some(name) => name.clone(),
        None => match rename_all {
            Some(rule) => names_::apply_rename_all_(rule, &base_name, field.span())?,
            None => base_name,
        },
    };
    let binding = ident.clone().unwrap_or_else(|| format_ident!("f{index}"));
    let member = match &ident {
        Some(ident) => Member::Named(ident.clone()),
        None => Member::Unnamed(Index {
            index: index as u32,
            span: field.span(),
        }),
    };
    let is_phantom = type_is_named_(&field.ty, "PhantomData");
    let is_option = type_is_named_(&field.ty, "Option");
    let skip_encoding = attrs.skip_encoding || is_phantom;
    let skip_decoding = attrs.skip_decoding || is_phantom;

    if !named && (skip_encoding || skip_decoding) {
        // 元组字段的跳过会改变位置语义，先明确拒绝，避免生成难以理解的代码。
        return Err(syn::Error::new_spanned(
            field,
            "denco derive 暂不支持跳过元组/元组变体字段",
        ));
    }

    Ok(FieldModel {
        ident_: ident,
        member_: member,
        binding_: binding,
        ty_: field.ty.clone(),
        name_: name,
        aliases_: attrs.aliases,
        skip_encoding_: skip_encoding,
        skip_decoding_: skip_decoding,
        default_: attrs.default,
        is_option_: is_option,
        is_phantom_: is_phantom,
    })
}

fn apply_rename_all_(attrs: &ContainerAttrs, ident: &Ident, span: Span) -> syn::Result<String> {
    let base = ident.to_string();
    let base = base.strip_prefix("r#").unwrap_or(&base);
    match attrs.rename_all.as_deref() {
        Some(rule) => names_::apply_rename_all_(rule, base, span),
        None => Ok(base.to_owned()),
    }
}

fn type_is_named_(ty: &Type, name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}
