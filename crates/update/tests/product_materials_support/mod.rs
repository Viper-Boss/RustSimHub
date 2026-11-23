// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic signed inputs reused by the real package/executor test targets.
//! Nothing here asserts installation permission or uses a production key.
#![allow(dead_code)]
pub mod timestamps;
use crate::product_materials::{
    material_sha256, product_signing_bytes, ExpectedBindings, MaterialEntry, ProductManifestV1,
    Profile, TargetId, VerifiedProductMaterials,
};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    fs::{self, File},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

pub const NKN_GID: u32 = 1201;
pub const BLE_GID: u32 = 1202;
pub const FIXTURE_ROLE_POLICY: &[u8] =
    br#"{"format":1,"roles":{"nkn":{"uid":1201,"gid":1201},"ble":{"uid":1202,"gid":1202}}}"#;

pub struct Bundle {
    pub directory: TempDir,
    pub key: SigningKey,
    pub bindings: ExpectedBindings,
    pub manifest: ProductManifestV1,
}
impl Bundle {
    pub fn new(profile: Profile) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let bindings = ExpectedBindings {
            profile,
            source_commit: "a".repeat(40),
            source_archive_sha256: "b".repeat(64),
            application_manifest_sha256: "c".repeat(64),
            core_sha256: "d".repeat(64),
            approved_role_policy_sha256: material_sha256(FIXTURE_ROLE_POLICY),
        };
        let key = SigningKey::from_bytes(&[93; 32]); // Test-only, never device signing trust.
        let mut files = Vec::new();
        for target in profile.targets() {
            let bytes = payload(target.id, &bindings);
            put(&directory.path().join(target.id.payload_name()), &bytes);
            files.push(MaterialEntry {
                id: target.id,
                bytes: bytes.len() as u64,
                sha256: material_sha256(&bytes),
            });
        }
        let manifest = ProductManifestV1 {
            format: 1,
            kind: "product-maintenance".into(),
            board: "ufi003-v02".into(),
            arch: "aarch64-linux".into(),
            bindings: bindings.clone(),
            roles: profile.required_roles().to_vec(),
            files,
        };
        let bundle = Self {
            directory,
            key,
            bindings,
            manifest,
        };
        bundle.sign(&bundle.manifest.encode().unwrap());
        bundle
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
    pub fn fd(&self) -> File {
        File::open(self.path()).unwrap()
    }
    pub fn verify(&self) -> crate::product_materials::Result<VerifiedProductMaterials> {
        VerifiedProductMaterials::open(
            &self.fd(),
            &self.key.verifying_key().to_bytes(),
            &self.bindings,
        )
    }
    pub fn sign(&self, raw: &[u8]) {
        put(&self.path().join("manifest.json"), raw);
        put(
            &self.path().join("manifest.sig"),
            &self
                .key
                .sign(&product_signing_bytes(raw).unwrap())
                .to_bytes(),
        );
    }
    pub fn sign_domain(&self, raw: &[u8], domain: &[u8]) {
        let mut message = domain.to_vec();
        message.extend_from_slice(raw);
        put(&self.path().join("manifest.json"), raw);
        put(
            &self.path().join("manifest.sig"),
            &self.key.sign(&message).to_bytes(),
        );
    }
    pub fn resign(&self) {
        self.sign(&serde_json::to_vec(&self.manifest).unwrap());
    }
    pub fn replace_payload(&mut self, id: TargetId, bytes: &[u8]) {
        put(&self.path().join(id.payload_name()), bytes);
        let entry = self
            .manifest
            .files
            .iter_mut()
            .find(|entry| entry.id == id)
            .unwrap();
        entry.bytes = bytes.len() as u64;
        entry.sha256 = material_sha256(bytes);
        self.resign();
    }
    pub fn public(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    pub fn source_files(&self) -> Vec<(String, Vec<u8>)> {
        fs::read_dir(self.path())
            .unwrap()
            .map(|entry| {
                let p = entry.unwrap().path();
                (
                    p.file_name().unwrap().to_str().unwrap().into(),
                    fs::read(p).unwrap(),
                )
            })
            .collect()
    }
}
pub fn put(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
pub fn elf() -> Vec<u8> {
    let mut bytes = vec![0; 64];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[16] = 2;
    bytes[18] = 183;
    bytes[52] = 64;
    bytes
}
pub fn payload(id: TargetId, bindings: &ExpectedBindings) -> Vec<u8> {
    let material =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../firmware/system/product-profiles");
    if id.spec().executable {
        return elf();
    }
    match id {
        TargetId::ProfileConfig => serde_json::to_vec(&serde_json::json!({"format":1,
            "profile":bindings.profile,"source_archive_sha256":bindings.source_archive_sha256,
            "core_sha256":bindings.core_sha256}))
        .unwrap(),
        TargetId::CoreDropin => fs::read_to_string(material.join("50-product-profile.conf"))
            .unwrap()
            .replace(
                "@CORE_MEMORY@",
                if bindings.profile.product() {
                    "192M"
                } else {
                    "96M"
                },
            )
            .replace(
                "@CORE_TASKS@",
                if bindings.profile.product() {
                    "64"
                } else {
                    "16"
                },
            )
            .into_bytes(),
        TargetId::NknUnit => fs::read(material.join("rustsimhub-nkn-root.service")).unwrap(),
        TargetId::BleUnit => fs::read(material.join("rustsimhub-ble-recovery.service")).unwrap(),
        TargetId::BlePolicy => fs::read(material.join("ble-product.policy")).unwrap(),
        TargetId::HealthDropin => fs::read(material.join("55-product-health.conf")).unwrap(),
        TargetId::HealthTmpfiles => fs::read(
            material
                .parent()
                .unwrap()
                .join("update-health/tmpfiles.conf"),
        )
        .unwrap(),
        TargetId::HealthService => fs::read(
            material
                .parent()
                .unwrap()
                .join("update-health/rustsimhub-update-health.service"),
        )
        .unwrap(),
        TargetId::HealthTimer => fs::read(
            material
                .parent()
                .unwrap()
                .join("update-health/rustsimhub-update-health.timer"),
        )
        .unwrap(),
        TargetId::ProductTmpfiles => fs::read_to_string(material.join("product.tmpfiles.in"))
            .unwrap()
            .replace("@NKN_GID@", &NKN_GID.to_string())
            .replace("@BLE_GID@", &BLE_GID.to_string())
            .into_bytes(),
        _ => unreachable!("exhaustive fixed payload table"),
    }
}
