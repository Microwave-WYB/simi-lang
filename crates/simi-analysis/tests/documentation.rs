use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use simi_analysis::{AnalysisDatabase, infer_types, module_shape, parse};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn tour_pages() -> Vec<PathBuf> {
    let directory = repository_root().join("docs/language-tour");
    let mut pages = fs::read_dir(directory)
        .expect("language tour directory")
        .map(|entry| entry.expect("tour directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    pages.sort();
    pages
}

fn markdown_links(markdown: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            let destination = &rest[open + 2..];
            let Some(close) = destination.find(')') else {
                break;
            };
            links.push(destination[..close].to_owned());
            rest = &destination[close + 1..];
        }
    }
    links
}

fn markdown_anchor(title: &str) -> String {
    let mut anchor = String::new();
    let mut pending_dash = false;
    for character in title.chars().filter(|character| *character != '`') {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
            if pending_dash && !anchor.is_empty() {
                anchor.push('-');
            }
            pending_dash = false;
            anchor.push(character.to_ascii_lowercase());
        } else if character.is_ascii_whitespace() {
            pending_dash = true;
        }
    }
    anchor
}

fn highlighted_simi_fences(markdown: &str, page: &Path) -> Vec<String> {
    let mut snippets = Vec::new();
    let mut current = None::<String>;
    for line in markdown.lines() {
        if line == "```elixir" {
            assert!(current.is_none(), "nested Simi fence in {}", page.display());
            current = Some(String::new());
        } else if line == "```" {
            if let Some(source) = current.take() {
                snippets.push(source);
            }
        } else if let Some(source) = &mut current {
            source.push_str(line);
            source.push('\n');
        }
    }
    assert!(
        current.is_none(),
        "unterminated Simi fence in {}",
        page.display()
    );
    snippets
}

fn module_catalog(db: &AnalysisDatabase) -> HashMap<String, simi_analysis::ModuleShape> {
    [
        ("std/list", include_str!("../../../stdlib/list.simi")),
        ("std/map", include_str!("../../../stdlib/map.simi")),
        ("std/iter", include_str!("../../../stdlib/iter.simi")),
        ("std/number", include_str!("../../../stdlib/number.simi")),
        ("std/string", include_str!("../../../stdlib/string.simi")),
        ("std/io", include_str!("../../../stdlib/io.simi")),
    ]
    .into_iter()
    .map(|(name, source)| {
        let file = db.add_file(source);
        (name.to_owned(), module_shape(db, file))
    })
    .collect()
}

#[test]
fn readme_and_every_tour_page_have_independently_validated_simi_scripts() {
    let db = AnalysisDatabase::default();
    let modules = module_catalog(&db);
    let mut pages = tour_pages();
    assert!(!pages.is_empty(), "language tour has no content pages");
    pages.push(repository_root().join("README.md"));

    for page in pages {
        let markdown = fs::read_to_string(&page).expect("tour page");
        let snippets = highlighted_simi_fences(&markdown, &page);
        assert!(
            !snippets.is_empty(),
            "{} has no highlighted Simi scripts",
            page.display()
        );
        for (index, source) in snippets.iter().enumerate() {
            let file = db.add_file(source);
            let syntax = parse(&db, file);
            assert!(
                syntax.diagnostics.is_empty(),
                "{} fence {} has syntax diagnostics: {:?}\n{}",
                page.display(),
                index + 1,
                syntax.diagnostics,
                source
            );
            let inference = infer_types(&db, file, &modules);
            let expects_diagnostic = source
                .lines()
                .any(|line| line.starts_with("-- Expected type"));
            if expects_diagnostic {
                assert!(
                    !inference.diagnostics.is_empty(),
                    "{} fence {} must produce its documented type diagnostic\n{}",
                    page.display(),
                    index + 1,
                    source
                );
            } else {
                assert!(
                    inference.diagnostics.is_empty(),
                    "{} fence {} has type diagnostics: {:?}\n{}",
                    page.display(),
                    index + 1,
                    inference.diagnostics,
                    source
                );
            }
        }
    }
}

#[test]
fn tour_contents_and_navigation_use_complete_unnumbered_topic_links() {
    let root = repository_root();
    let landing_path = root.join("docs/language-tour.md");
    let landing = fs::read_to_string(&landing_path).expect("tour landing page");
    let pages = tour_pages();
    let filenames = pages
        .iter()
        .map(|page| page.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<HashSet<_>>();

    assert!(filenames.iter().all(|name| {
        !name
            .split_once('-')
            .is_some_and(|(prefix, _)| prefix.chars().all(|character| character.is_ascii_digit()))
    }));

    let landing_targets = markdown_links(&landing)
        .into_iter()
        .filter_map(|target| target.strip_prefix("language-tour/").map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(
        landing_targets.iter().cloned().collect::<HashSet<_>>(),
        filenames,
        "landing page must link every tour topic exactly once"
    );
    assert_eq!(landing_targets.len(), filenames.len());

    for (index, filename) in landing_targets.iter().enumerate() {
        let page_path = root.join("docs/language-tour").join(filename);
        let markdown = fs::read_to_string(&page_path).expect("linked tour page");
        let title = markdown
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("# "))
            .expect("tour page title");
        let toc = markdown
            .split_once("## Tour contents\n")
            .and_then(|(_, rest)| rest.split_once("\n## ").map(|(toc, _)| toc))
            .expect("tour contents section");

        assert!(
            toc.lines().any(|line| line == format!("- {title}")),
            "{} must render its own title as plain text",
            page_path.display()
        );
        for sibling in &landing_targets {
            if sibling != filename {
                assert!(
                    toc.contains(&format!("]({sibling})")),
                    "{} must link sibling {sibling}",
                    page_path.display()
                );
            }
        }
        assert!(
            !toc.lines().any(|line| {
                line.trim_start()
                    .split_once('.')
                    .is_some_and(|(prefix, _)| {
                        prefix.chars().all(|character| character.is_ascii_digit())
                    })
            }),
            "{} must use an unnumbered tour contents list",
            page_path.display()
        );
        let body = markdown
            .split_once("<!-- tour:contents:end -->")
            .map(|(_, body)| body)
            .expect("generated contents end marker");
        for heading in body.lines().filter_map(|line| {
            line.strip_prefix("## ")
                .or_else(|| line.strip_prefix("### "))
        }) {
            let link = format!("[{heading}](#{})", markdown_anchor(heading));
            assert!(
                toc.contains(&link),
                "{} must link its own subsection {heading}",
                page_path.display()
            );
        }

        let previous = index.checked_sub(1).map(|index| &landing_targets[index]);
        let next = landing_targets.get(index + 1);
        assert_eq!(markdown.contains("[Previous:"), previous.is_some());
        assert_eq!(markdown.contains("[Next:"), next.is_some());
        if let Some(previous) = previous {
            assert!(markdown.contains(&format!("]({previous})")));
        }
        if let Some(next) = next {
            assert!(markdown.contains(&format!("]({next})")));
        }
    }
}

#[test]
fn readme_and_tour_local_links_resolve() {
    let root = repository_root();
    let mut pages = vec![root.join("README.md"), root.join("docs/language-tour.md")];
    pages.extend(tour_pages());

    for page in pages {
        let markdown = fs::read_to_string(&page).expect("Markdown page");
        for target in markdown_links(&markdown) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.starts_with('#')
            {
                continue;
            }
            let path = target.split('#').next().unwrap_or_default();
            let resolved = page.parent().unwrap().join(path);
            assert!(
                resolved.exists(),
                "{} links to missing local target {target}",
                page.display()
            );
        }
    }
}
