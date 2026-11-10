//! The asset read: a bikeshed-relative image path answers its confined absolute
//! file, its media type and its byte size. The path-hygiene rule is
//! `BikeshedPath::url`'s, so this suite pins the seams the protocol handler
//! leans on: an escape is `InvalidName`, a missing file or a directory is
//! `UnknownNote`, and anything that is not an image is `NotAnAsset`.

use std::fs;

use crate::bikeshed::Bikeshed;
use crate::domain::error::DomainError;

use super::bikeshed_support::Vault;

#[test]
fn a_staged_image_answers_its_path_file_type_and_size() {
    let vault = Vault::new("asset-answers");
    vault.stage("img/photo.PNG", "png-bytes");

    let asset = vault
        .bikeshed()
        .asset("img/photo.PNG")
        .expect("the image answers")
        .value;

    assert_eq!(asset.path, "img/photo.PNG");
    assert_eq!(asset.file, vault.dir().join("img").join("photo.PNG"));
    assert_eq!(asset.media_type, "image/png");
    assert_eq!(asset.size, 9);
}

#[test]
fn every_image_extension_maps_case_insensitively() {
    let vault = Vault::new("asset-table");
    let table = [
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("webp", "image/webp"),
        ("avif", "image/avif"),
        ("svg", "image/svg+xml"),
        ("bmp", "image/bmp"),
        ("ico", "image/x-icon"),
    ];
    for (extension, media_type) in table {
        for spelling in [extension.to_string(), extension.to_uppercase()] {
            let path = format!("photo.{spelling}");
            vault.stage(&path, "bytes");
            let asset = vault
                .bikeshed()
                .asset(&path)
                .unwrap_or_else(|error| panic!("{path} answers: {error:?}"))
                .value;
            assert_eq!(asset.media_type, media_type, "{path} maps");
        }
    }
}

#[test]
fn anything_but_an_image_is_refused_as_not_an_asset() {
    let vault = Vault::new("asset-not-image");
    for path in ["note.md", "photo.pdf", "photo", "photo.png.txt"] {
        vault.stage(path, "bytes");
        assert_eq!(
            vault.bikeshed().asset(path),
            Err(DomainError::NotAnAsset(path.to_string())),
            "{path} is refused"
        );
    }
}

#[test]
fn a_directory_is_unknown_even_when_it_names_an_image() {
    let vault = Vault::new("asset-directory");
    fs::create_dir_all(vault.dir().join("dir.png")).expect("the folder is creatable");

    assert_eq!(
        vault.bikeshed().asset("dir.png"),
        Err(DomainError::UnknownNote("dir.png".to_string()))
    );
}

#[test]
fn the_path_hygiene_rule_refuses_escapes_before_the_file_is_read() {
    let vault = Vault::new("asset-escape");
    for bad in [
        "../escape.png",
        "/etc/passwd.png",
        "C:\\evil.png",
        "a\\b.png",
        "a/./b.png",
        "a//b.png",
    ] {
        assert_eq!(
            vault.bikeshed().asset(bad),
            Err(DomainError::InvalidName(bad.to_string())),
            "{bad} is refused"
        );
    }
}

#[test]
fn a_missing_file_is_unknown() {
    let vault = Vault::new("asset-missing");
    assert_eq!(
        Bikeshed::new(vault.dir()).asset("missing.png"),
        Err(DomainError::UnknownNote("missing.png".to_string()))
    );
}
