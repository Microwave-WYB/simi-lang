use super::*;

pub fn module_at(
    db: &dyn salsa::Database,
    file: FileId,
    modules: &HashMap<String, ModuleShape>,
    offset: usize,
) -> Option<ModuleValue> {
    let parsed = parse(db, file);
    let resolution = resolve(db, file);
    let bindings = known_bindings(db, file);

    if let Some(symbol) = resolution.symbol_at(offset)
        && let Some(value) = bindings.get(&symbol)
        && value.path.is_empty()
    {
        return Some(ModuleValue {
            module: value.module.clone(),
            documentation: modules.get(&value.module)?.documentation.clone(),
        });
    }

    let module = parsed
        .syntax()
        .descendants()
        .filter_map(syntax::CallExpr::cast)
        .find_map(|call| {
            let arguments = support::child::<syntax::ArgList>(call.syntax())?;
            let literal = support::children::<syntax::Expr>(arguments.syntax()).next()?;
            let syntax::Expr::Literal(literal) = literal else {
                return None;
            };
            let token = support::token(literal.syntax(), K::STRING)?;
            contains(token_span(&token), offset)
                .then(|| required_module(&call, &resolution))
                .flatten()
        })?;
    Some(ModuleValue {
        documentation: modules.get(&module)?.documentation.clone(),
        module,
    })
}
pub fn imported_members(
    db: &dyn salsa::Database,
    file: FileId,
    modules: &HashMap<String, ModuleShape>,
) -> HashMap<SymbolId, ModuleMember> {
    let resolution = resolve(db, file);
    known_bindings(db, file)
        .into_iter()
        .filter_map(|(symbol, value)| {
            let mut member = member_from_value(modules, &value)?;
            member.field.name = resolution.symbol_data(symbol)?.name.clone();
            Some((symbol, member))
        })
        .collect()
}
pub fn member_at(
    db: &dyn salsa::Database,
    file: FileId,
    modules: &HashMap<String, ModuleShape>,
    _source: &str,
    offset: usize,
) -> Option<ModuleMember> {
    let parsed = parse(db, file);
    let resolution = resolve(db, file);
    let bindings = known_bindings(db, file);

    let field = parsed
        .syntax()
        .descendants()
        .filter_map(syntax::FieldExpr::cast)
        .filter_map(|field| {
            let token = support::token(field.syntax(), K::IDENT)?;
            contains(token_span(&token), offset).then_some(field)
        })
        .min_by_key(|field| field.syntax().text_range().len());
    if let Some(field) = field
        && let Some(value) = known_value(syntax::Expr::Field(field), &resolution, &bindings)
    {
        return member_from_value(modules, &value);
    }

    let symbol = resolution.symbol_at(offset)?;
    let mut member = member_from_value(modules, bindings.get(&symbol)?)?;
    member.field.name = resolution.symbol_data(symbol)?.name.clone();
    Some(member)
}
pub fn member_completions(
    db: &dyn salsa::Database,
    file: FileId,
    modules: &HashMap<String, ModuleShape>,
    source: &str,
    offset: usize,
) -> Vec<ExportField> {
    let parsed = parse(db, file);
    let resolution = resolve(db, file);
    let bindings = known_bindings(db, file);

    if let Some((value, prefix)) =
        completion_value(&parsed.syntax(), offset, &resolution, &bindings)
    {
        return completions_from_value(modules, &value, &prefix);
    }

    let Some((receiver, path, prefix)) = member_path(source, offset, true) else {
        return Vec::new();
    };
    let Some(symbol) = resolution.symbol_at(receiver.1) else {
        return Vec::new();
    };
    let Some(mut value) = bindings.get(&symbol).cloned() else {
        return Vec::new();
    };
    value.path.extend(path);
    completions_from_value(modules, &value, &prefix)
}
pub(super) fn member_from_value(
    modules: &HashMap<String, ModuleShape>,
    value: &KnownValue,
) -> Option<ModuleMember> {
    let (field, parents) = value.path.split_last()?;
    let fields = descend_fields(&modules.get(&value.module)?.fields, parents)?;
    Some(ModuleMember {
        module: value.module.clone(),
        field: fields
            .iter()
            .find(|candidate| &candidate.name == field)?
            .clone(),
    })
}
pub(super) fn completions_from_value(
    modules: &HashMap<String, ModuleShape>,
    value: &KnownValue,
    prefix: &str,
) -> Vec<ExportField> {
    let Some(shape) = modules.get(&value.module) else {
        return Vec::new();
    };
    descend_fields(&shape.fields, &value.path).map_or_else(Vec::new, |fields| {
        fields
            .iter()
            .filter(|field| field.name.starts_with(prefix))
            .cloned()
            .collect()
    })
}
pub(super) fn completion_value(
    root: &SyntaxNode,
    offset: usize,
    resolution: &Resolution,
    bindings: &HashMap<SymbolId, KnownValue>,
) -> Option<(KnownValue, String)> {
    root.descendants()
        .filter_map(syntax::FieldExpr::cast)
        .filter(|field| {
            let range = field.syntax().text_range();
            let start = u32::from(range.start()) as usize;
            let end = u32::from(range.end()) as usize;
            start <= offset && offset <= end
        })
        .min_by_key(|field| field.syntax().text_range().len())
        .and_then(|field| {
            let base = field.syntax().children().find_map(syntax::Expr::cast)?;
            let value = known_value(base, resolution, bindings)?;
            let prefix = support::token(field.syntax(), K::IDENT)
                .map(|token| token.text().to_owned())
                .unwrap_or_default();
            Some((value, prefix))
        })
}
pub(super) fn descend_fields<'a>(
    mut fields: &'a [ExportField],
    path: &[String],
) -> Option<&'a [ExportField]> {
    for segment in path {
        fields = &fields.iter().find(|field| &field.name == segment)?.fields;
    }
    Some(fields)
}
pub(super) fn member_path(
    source: &str,
    offset: usize,
    completion: bool,
) -> Option<((String, usize), Vec<String>, String)> {
    source.get(..offset)?;
    let mut end = offset;
    if !completion {
        end += source[offset..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
            .map(char::len_utf8)
            .sum::<usize>();
    }
    let start = source[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '.'
        })
        .last()
        .map(|(index, _)| index)?;
    let access = source.get(start..end)?;
    let mut segments = access.split('.').map(str::to_owned).collect::<Vec<_>>();
    if segments.len() < 2 {
        return None;
    }
    let field = segments.pop()?;
    if !completion && field.is_empty() {
        return None;
    }
    let receiver = segments.remove(0);
    if receiver.is_empty() || segments.iter().any(String::is_empty) {
        return None;
    }
    Some(((receiver, start), segments, field))
}
