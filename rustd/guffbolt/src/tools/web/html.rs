//! Render a single parsed document and retain its structured data.

const TEXT_WIDTH: usize = 200;
const JSON_LD: &str = "application/ld+json";

pub(super) fn markdown(input: &str) -> Result<String, html2text::Error> {
    let config = html2text::config::plain();
    let dom = config.parse_html(input.as_bytes())?;
    let tree = config.dom_to_render_tree(&dom)?;
    let mut text = config.render_to_string(tree, TEXT_WIDTH)?;
    append_structured_data(&mut text, &dom.document)?;
    Ok(text)
}

// Keep the document alive for the entire walk. html2text's DOM tears down a
// dropped node's descendants iteratively, including their child lists.
fn append_structured_data(
    text: &mut String,
    document: &html2text::Handle,
) -> Result<(), html2text::Error> {
    let mut nodes = vec![std::rc::Rc::clone(document)];
    while let Some(node) = nodes.pop() {
        nodes.extend(node.children.borrow().iter().rev().cloned());
        let html2text::Element { name, attrs, .. } = &node.data else {
            continue;
        };
        if &*name.local != "script"
            || !attrs.borrow().iter().any(|attr| {
                &*attr.name.local == "type" && attr.value.trim().eq_ignore_ascii_case(JSON_LD)
            })
        {
            continue;
        }
        // Serialize the parsed raw-text element to keep literal &, < and > in the JSON.
        // Attributes are no longer needed after rendering. Removing them makes the
        // serializer's enclosing tags fixed, regardless of the original spelling.
        attrs.borrow_mut().clear();
        let mut bytes = Vec::new();
        node.serialize(&mut bytes)?;
        let raw = String::from_utf8_lossy(&bytes);
        if let Some(json) = raw
            .strip_prefix("<script>")
            .and_then(|script| script.strip_suffix("</script>"))
        {
            text.push_str("\n\n## JSON-LD\n\n");
            text.push_str(json.trim());
        }
    }
    Ok(())
}
