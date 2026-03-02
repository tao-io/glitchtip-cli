use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::Serialize;
use symbolic::common::{ByteView, DebugId};
use symbolic::debuginfo::sourcebundle::SourceBundleWriter;
use symbolic::debuginfo::{Archive, Object};

use crate::api::ApiClient;
use crate::api::client::{DifAssembleFile, DifAssembleRequest};
use crate::config::Config;
use crate::utils::dif::{DifFile, DifType};
use crate::utils::{gzip_compress, sha1_hex};

pub fn check(path: &Path, ty: Option<&str>, json: bool) -> Result<()> {
    if path.is_dir() {
        anyhow::bail!(
            "This command can only check individual debug files, \
            but the provided path leads to a directory."
        );
    }

    let ty: Option<DifType> = ty.map(|t| t.parse()).transpose()?;
    let dif = DifFile::open_path(path, ty)?;

    if json {
        serde_json::to_writer_pretty(&mut io::stdout(), &dif)?;
        println!();
        return if dif.is_usable() {
            Ok(())
        } else {
            anyhow::bail!("")
        };
    }

    println!("Debug Info File Check");
    match dif.kind() {
        Some(class) => println!("  Type: {} {class:#}", dif.ty()),
        None => println!("  Type: {}", dif.ty()),
    }

    println!("  Contained debug identifiers:");
    for variant in dif.variants() {
        println!("    > Debug ID: {}", variant.debug_id);
        if let Some(code_id) = variant.code_id {
            println!("      Code ID:  {code_id}");
        }
        if let Some(arch) = variant.arch {
            println!("      Arch:     {arch}");
        }
    }

    println!("  Contained debug information:");
    println!("    > {}", dif.features());

    if let Some(prob) = dif.get_problem() {
        println!("  Usable: no ({prob})");
        anyhow::bail!("")
    } else {
        println!("  Usable: yes");
        Ok(())
    }
}

// --- upload ---

pub struct UploadOptions<'a> {
    pub paths: &'a [PathBuf],
    pub ty: Option<&'a str>,
    #[allow(dead_code)] // Reserved for feature filtering
    pub no_unwind: bool,
    #[allow(dead_code)] // Reserved for feature filtering
    pub no_debug: bool,
    #[allow(dead_code)] // Reserved for feature filtering
    pub no_sources: bool,
    pub wait: bool,
}

struct FoundDif {
    path: PathBuf,
    debug_id: Option<String>,
}

pub async fn upload(config: &Config, opts: &UploadOptions<'_>) -> Result<()> {
    let org = config.get_org()?;
    let project = config.project.as_deref().context(
        "No project specified. Use --project, SENTRY_PROJECT, or set it in .sentryclirc",
    )?;

    let ty: Option<DifType> = opts.ty.map(|t| t.parse()).transpose()?;

    // Discover debug files
    let found = discover_difs(opts.paths, ty)?;

    if found.is_empty() {
        println!("No debug information files found.");
        return Ok(());
    }

    println!("Found {} debug information file(s)", found.len());

    let client = ApiClient::from_config(config)?;

    // Get chunk upload info
    let upload_info = client
        .get_chunk_upload_info(org)
        .await
        .context("Failed to get chunk upload info")?;

    let mut uploaded = 0;
    let mut errors = 0;

    // Upload each file
    let mut assemble_files: HashMap<String, DifAssembleFile> = HashMap::new();
    let mut checksum_to_name: HashMap<String, String> = HashMap::new();

    for dif_info in &found {
        let path = &dif_info.path;
        let display_name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string();

        // Read the raw file bytes
        let mut file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        // Compute SHA1 checksum of original file
        let checksum = sha1_hex(&data);

        // Gzip compress
        let compressed = gzip_compress(&data)?;
        // Upload chunk
        match client
            .upload_chunk(&upload_info.url, compressed, &checksum)
            .await
        {
            Ok(()) => {
                println!("  Uploaded chunk for: {display_name}");
                uploaded += 1;
            }
            Err(e) => {
                eprintln!("  Error uploading {display_name}: {e}");
                errors += 1;
                continue;
            }
        }

        checksum_to_name.insert(checksum.clone(), display_name.clone());
        assemble_files.insert(
            checksum.clone(),
            DifAssembleFile {
                name: display_name,
                debug_id: dif_info.debug_id.clone(),
                chunks: vec![checksum],
            },
        );
    }

    if assemble_files.is_empty() {
        println!("No files to assemble.");
        return Ok(());
    }

    // Call assemble endpoint
    let request = DifAssembleRequest {
        files: assemble_files,
    };

    println!("Requesting assembly of {} file(s)...", request.files.len());

    let response = client.assemble_difs(org, project, &request).await?;

    // Optionally wait/poll for assembly
    if opts.wait {
        poll_assembly(&client, org, project, &request, &checksum_to_name).await?;
    } else {
        // Just report initial status
        for (checksum, status) in &response.files {
            let name = checksum_to_name
                .get(checksum)
                .map(|s| s.as_str())
                .unwrap_or(checksum);
            match status.state.as_str() {
                "ok" | "created" => println!("  OK: {name}"),
                "error" => {
                    let detail = status.detail.as_deref().unwrap_or("unknown error");
                    eprintln!("  Error: {name}: {detail}");
                    errors += 1;
                }
                state => println!("  {name}: {state}"),
            }
        }
    }

    println!();
    println!("Upload complete: {uploaded} chunk(s) uploaded, {errors} error(s).");

    if errors > 0 {
        anyhow::bail!("{errors} file(s) had errors");
    }

    Ok(())
}

async fn poll_assembly(
    client: &ApiClient,
    org: &str,
    project: &str,
    request: &DifAssembleRequest,
    checksum_to_name: &HashMap<String, String>,
) -> Result<()> {
    let max_polls = 60;
    let poll_interval = std::time::Duration::from_secs(1);

    for _ in 0..max_polls {
        tokio::time::sleep(poll_interval).await;

        let response = client.assemble_difs(org, project, request).await?;

        let mut all_done = true;
        for (checksum, status) in &response.files {
            let name = checksum_to_name
                .get(checksum)
                .map(|s| s.as_str())
                .unwrap_or(checksum);
            match status.state.as_str() {
                "ok" | "created" => {}
                "error" => {
                    let detail = status.detail.as_deref().unwrap_or("unknown error");
                    eprintln!("  Error: {name}: {detail}");
                }
                _ => {
                    all_done = false;
                }
            }
        }

        if all_done {
            println!("  Assembly completed.");
            return Ok(());
        }
    }

    println!("  Assembly did not complete within timeout.");
    Ok(())
}

fn discover_difs(paths: &[PathBuf], ty: Option<DifType>) -> Result<Vec<FoundDif>> {
    let mut found = Vec::new();

    for path in paths {
        if path.is_file() {
            if let Some(dif) = try_open_dif(path, ty) {
                found.push(dif);
            }
        } else if path.is_dir() {
            walk_difs(path, ty, &mut found)?;
        } else {
            eprintln!("Warning: {} does not exist, skipping", path.display());
        }
    }

    Ok(found)
}

fn walk_difs(dir: &Path, ty: Option<DifType>, found: &mut Vec<FoundDif>) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if let Some(dif) = try_open_dif(entry.path(), ty) {
            found.push(dif);
        }
    }

    Ok(())
}

fn try_open_dif(path: &Path, ty: Option<DifType>) -> Option<FoundDif> {
    let dif = DifFile::open_path(path, ty).ok()?;

    if !dif.is_usable() {
        return None;
    }

    // Get the first debug ID
    let debug_id = dif.ids().next().map(|id| id.to_string());

    Some(FoundDif {
        path: path.to_path_buf(),
        debug_id,
    })
}

// --- find ---

#[derive(Serialize)]
struct FindMatch {
    #[serde(rename = "type")]
    ty: DifType,
    id: DebugId,
    path: PathBuf,
}

pub fn find(
    ids: &[String],
    types: &[String],
    paths: &[PathBuf],
    json: bool,
    no_well_known: bool,
    no_cwd: bool,
) -> Result<()> {
    if ids.is_empty() {
        println!("No debug IDs specified.");
        return Ok(());
    }

    let search_ids: HashSet<DebugId> = ids
        .iter()
        .map(|s| DebugId::from_str(s))
        .collect::<Result<_, _>>()
        .context("Invalid debug ID")?;

    let filter_types: HashSet<DifType> = if types.is_empty() {
        DifType::all().iter().copied().collect()
    } else {
        types.iter().map(|s| s.parse()).collect::<Result<_, _>>()?
    };

    let mut search_paths: HashSet<PathBuf> = paths.iter().cloned().collect();

    if !no_cwd && let Ok(cwd) = std::env::current_dir() {
        search_paths.insert(cwd);
    }

    if !no_well_known
        && filter_types.contains(&DifType::Dsym)
        && let Some(path) = dirs::home_dir().map(|h| h.join("Library/Developer/Xcode/DerivedData"))
        && path.is_dir()
    {
        search_paths.insert(path);
    }

    let mut remaining = search_ids.clone();
    let mut found_files: Vec<FindMatch> = Vec::new();

    let iter = search_paths
        .iter()
        .flat_map(walkdir::WalkDir::new)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file());

    for dirent in iter {
        if remaining.is_empty() {
            break;
        }

        for &ty in &filter_types {
            let matches = find_ids_for_type(dirent.path(), ty, &remaining);
            for (id, matched_ty) in matches {
                found_files.push(FindMatch {
                    ty: matched_ty,
                    id,
                    path: dirent.path().to_path_buf(),
                });
                remaining.remove(&id);
            }
        }
    }

    if json {
        serde_json::to_writer_pretty(&mut io::stdout(), &found_files)?;
        println!();
    } else {
        for m in &found_files {
            println!("{} {} [{}]", m.id, m.path.display(), m.ty);
        }
        if !remaining.is_empty() {
            eprintln!();
            eprintln!("missing debug information files:");
            for id in &remaining {
                eprintln!("  {id} ({})", id_hint(id));
            }
        }
    }

    if remaining.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("")
    }
}

fn id_hint(id: &DebugId) -> &'static str {
    if id.appendix() > 0 {
        return "likely PDB";
    }

    match id.uuid().get_version() {
        Some(uuid::Version::Sha1) => "likely Proguard",
        Some(uuid::Version::Md5) => "likely dSYM",
        None => "likely ELF Debug",
        _ => "unknown",
    }
}

fn find_ids_for_type(
    path: &Path,
    ty: DifType,
    remaining: &HashSet<DebugId>,
) -> Vec<(DebugId, DifType)> {
    let ext = path.extension().and_then(|e| e.to_str());

    // Quick extension-based filtering per type
    match ty {
        DifType::Dsym => {
            if ext == Some("class") {
                return vec![];
            }
        }
        DifType::Pe => {
            if ext != Some("exe") && ext != Some("dll") {
                return vec![];
            }
        }
        DifType::Pdb | DifType::PortablePdb => {
            if ext != Some("pdb") {
                return vec![];
            }
        }
        DifType::SourceBundle => {
            if ext != Some("zip") {
                return vec![];
            }
        }
        DifType::Breakpad => {
            if ext != Some("sym") {
                return vec![];
            }
        }
        DifType::Proguard => {
            if ext != Some("txt") {
                return vec![];
            }
        }
        _ => {}
    }

    let Ok(dif) = DifFile::open_path(path, Some(ty)) else {
        return vec![];
    };

    dif.ids()
        .filter(|id| remaining.contains(id))
        .map(|id| (id, ty))
        .collect()
}

// --- print-sources ---

pub fn print_sources(path: &Path) -> Result<()> {
    let data =
        ByteView::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let archive =
        Archive::parse(&data).with_context(|| format!("Failed to parse {}", path.display()))?;

    if archive.object_count() == 0 {
        println!("No objects found in the given debug info file");
        return Ok(());
    }

    for object in archive.objects() {
        let object = object?;
        print_object_sources(&object)?;
    }

    Ok(())
}

fn print_object_sources(object: &Object) -> Result<()> {
    let debug_session = object.debug_session()?;

    if debug_session.files().next().is_none() {
        println!(
            "{} {} has no sources.",
            object.file_format(),
            object.debug_id()
        );
    } else {
        println!(
            "{} {} references sources:",
            object.file_format(),
            object.debug_id()
        );
        for file in debug_session.files() {
            let file = file?;
            let abs_path = file.abs_path_str();
            println!("  {abs_path}");
            let source = debug_session.source_by_path(abs_path.as_str())?;
            match source {
                Some(source) => {
                    print_detail(
                        "Embedded",
                        source.contents().map(|v| format!("{} bytes", v.len())),
                    );
                    print_detail("Url", source.url());
                    print_detail("DebugId", source.debug_id());
                    print_detail("SourceMap Url", source.source_mapping_url());
                }
                None => {
                    if Path::new(&abs_path).exists() {
                        println!("    Not embedded, but available on the local disk.");
                    } else {
                        println!("    Not embedded nor available locally at the referenced path.");
                    }
                }
            }
        }
    }
    Ok(())
}

fn print_detail<T: fmt::Display>(name: &str, value: Option<T>) {
    if let Some(value) = value {
        println!("    {name}: {value}");
    }
}

// --- bundle-sources ---

pub fn bundle_sources(paths: &[PathBuf], output: Option<&Path>) -> Result<()> {
    for orig_path in paths {
        let canonical_path = get_canonical_path(orig_path)?;

        let archive = match DifFile::open_path(&canonical_path, None)? {
            DifFile::Archive(archive) => archive,
            _ => {
                eprintln!(
                    "Warning: Cannot build source bundles from {}, skipping",
                    orig_path.display()
                );
                continue;
            }
        };

        let parent_path = canonical_path.parent().unwrap_or(Path::new("."));
        let filename = canonical_path
            .file_name()
            .unwrap_or(canonical_path.as_os_str());

        for (index, object) in archive.get().objects().enumerate() {
            let object = object?;

            let mut out = output
                .unwrap_or(parent_path)
                .join(orig_path.file_name().unwrap_or(orig_path.as_os_str()));
            match index {
                0 => out.set_extension("src.zip"),
                index => out.set_extension(format!("{index}.src.zip")),
            };

            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }

            let writer = SourceBundleWriter::create(&out)?;

            let written =
                writer
                    .write_object_with_filter(&object, &filename.to_string_lossy(), |_, _| true)?;

            if !written {
                eprintln!("skipped {} (no files found)", orig_path.display());
                let _ = fs::remove_file(&out);
            } else {
                println!("{}", out.display());
            }
        }
    }

    Ok(())
}

fn get_canonical_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let mut canonical_path = path
        .as_ref()
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", path.as_ref().display()))?;

    // Handle macOS dSYM bundle navigation
    if canonical_path.extension().is_some_and(|e| e == "dSYM")
        && let Some(dsym_name) = canonical_path.file_stem().map(|s| s.to_os_string())
    {
        let dsym_path = canonical_path
            .join("Contents/Resources/DWARF")
            .join(&dsym_name);
        if dsym_path.is_file() {
            canonical_path = dsym_path;
        }
    }

    Ok(canonical_path)
}

// --- bundle-jvm ---

pub fn bundle_jvm(path: &Path, output: &Path, debug_id: &str) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Given path does not exist: {}", path.display());
    }

    if !path.is_dir() {
        anyhow::bail!("Given path is not a directory: {}", path.display());
    }

    let debug_id = DebugId::from_str(debug_id).context("Invalid debug ID")?;
    let out = output.join(format!("{debug_id}.zip"));

    if !output.exists() {
        fs::create_dir_all(output)
            .with_context(|| format!("Failed to create output directory {}", output.display()))?;
    }

    // Collect Java/Kotlin source files
    let source_extensions = ["java", "kt", "scala", "groovy"];
    let mut sources: Vec<PathBuf> = Vec::new();

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let entry_path = entry.path();
        if let Some(ext) = entry_path.extension().and_then(|e| e.to_str())
            && source_extensions.contains(&ext)
        {
            sources.push(entry_path.to_path_buf());
        }
    }

    if sources.is_empty() {
        anyhow::bail!("No source files found in {}", path.display());
    }

    // Create source bundle as a ZIP file
    let zip_file =
        fs::File::create(&out).with_context(|| format!("Failed to create {}", out.display()))?;
    let mut zip = zip::ZipWriter::new(zip_file);
    let zip_options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Write manifest
    let manifest = serde_json::json!({
        "debug_id": debug_id.to_string(),
        "type": "source_bundle",
    });
    zip.start_file("manifest.json", zip_options)?;
    io::Write::write_all(
        &mut zip,
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )?;

    // Write source files
    for source_path in &sources {
        let relative = source_path.strip_prefix(path).unwrap_or(source_path);
        let relative_jvm = relative.with_extension(format!(
            "{}.jvm",
            relative.extension().unwrap_or_default().to_string_lossy()
        ));
        let entry_name = format!("~/{}", relative_jvm.to_string_lossy().replace('\\', "/"));

        let content = fs::read_to_string(source_path)
            .with_context(|| format!("Failed to read {}", source_path.display()))?;

        zip.start_file(entry_name, zip_options)?;
        io::Write::write_all(&mut zip, content.as_bytes())?;
    }

    zip.finish()?;

    println!("Created {}", out.display());
    Ok(())
}
