// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::product_materials::{IsolatedServiceState, ProductInstaller, Profile, TestBoundary};
use std::{os::unix::fs::PermissionsExt, rc::Rc};
#[path = "../../../tests/product_materials_support/mod.rs"]
mod support;
struct Authority {
    tx: String,
    profile: Profile,
    calls: Cell<usize>,
}
impl RetainedAuthority for Authority {
    fn check_source(&self) -> Result<()> {
        Ok(())
    }
    fn transaction(&self) -> &str {
        &self.tx
    }
    fn profile(&self) -> Profile {
        self.profile
    }
    fn revalidate(&self, _: FencePhase, _: &OwnedRootChanges<'_>) -> Result<()> {
        self.calls.set(self.calls.get() + 1);
        Ok(())
    }
    fn runtime_directory(&self, path: &str) -> Option<(u32, u32, u32)> {
        matches!(path, "/run/rustsimhub/nkn" | "/run/rustsimhub/recovery").then(|| {
            (
                rustix::process::geteuid().as_raw(),
                rustix::process::getegid().as_raw(),
                0o2750,
            )
        })
    }
}
fn put(root: &Path, path: &str, bytes: &[u8], mode: u32) {
    let path = root.join(path.trim_start_matches('/'));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn prepare(root: &Path, bundle: &support::Bundle) {
    // Root::isolated requires a private directory. tempfile follows the caller's
    // umask, so make this fixture independent of the runner's default permissions.
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    put(root,"/etc/passwd",b"root:x:0:0:root:/root:/bin/sh\nrustsimhub-core:x:986:986::/nonexistent:/usr/sbin/nologin\nrustsimhub-edge:x:985:985::/nonexistent:/usr/sbin/nologin\nrustsimhub-media:x:984:984::/nonexistent:/usr/sbin/nologin\nrustsimhub-forwarder:x:983:983::/nonexistent:/usr/sbin/nologin\n",0o644);
    put(root,"/etc/group",b"root:x:0:\nrustsimhub-core:x:986:\nrustsimhub-edge:x:985:\nrustsimhub-media:x:984:\nrustsimhub-forwarder:x:983:\naudio:x:29:rustsimhub-media\nrustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder\n",0o644);
    put(
        root,
        "/etc/nsswitch.conf",
        b"passwd: files\ngroup: files\n",
        0o644,
    );
    put(
        root,
        "/etc/rustsimhub/product-roles.json",
        support::FIXTURE_ROLE_POLICY,
        0o600,
    );
    ProductInstaller::prepare_isolated_accounts(root).unwrap();
    for (name, bytes) in bundle.source_files() {
        put(
            root,
            &format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox/{name}"),
            &bytes,
            0o600,
        );
    }
}
#[test]
fn same_original_business_and_outbox_flocks_survive_real_signed_installation() {
    for profile in [Profile::Legacy, Profile::Product, Profile::Email] {
        original_flocks_survive_installation(profile);
    }
}
fn original_flocks_survive_installation(profile: Profile) {
    let root = crate::admission::tests::fixture(Mode::Forwarding, true);
    let bundle = support::Bundle::new(profile);
    prepare(root.path(), &bundle);
    let authority = Authority {
        tx: bundle.verify().unwrap().manifest_sha256().into(),
        profile,
        calls: Cell::new(0),
    };
    let snapshot = crate::admission::tests::snapshot(root.path());
    let fence = DataFence::at(snapshot, &authority, root.path()).unwrap();
    let services = Rc::new(IsolatedServiceState {
        off: Cell::new(true),
        ..Default::default()
    });
    let mut installer = ProductInstaller::prepare_isolated(
        root.path(),
        &bundle.public(),
        &bundle.bindings,
        &fence,
        services,
        false,
    )
    .unwrap();
    let data = root.path().join(STATE);
    let watch = data.clone();
    installer.fault_at(move |phase, _| {
        if matches!(
            phase,
            TestBoundary::BeforeReplace | TestBoundary::AfterJournal | TestBoundary::AfterReload
        ) {
            assert!(rustsimhub_storage::Store::open(&watch).is_err());
            let lock = private_file(
                &watch.join("forwarding/.writer.lock"),
                rustix::process::geteuid().as_raw(),
                0o600,
                0,
            )
            .unwrap();
            assert!(fs2::FileExt::try_lock_exclusive(&lock).is_err());
        }
        Ok(())
    });
    assert_eq!(
        installer.apply().unwrap().state,
        crate::product_materials::InstallState::Applied
    );
    fence.with_snapshot(|s| s.revalidate()).unwrap();
    assert!(authority.calls.get() > 6);
    drop(installer);
    let same = fence.into_snapshot().unwrap();
    // Descriptors outlive both the executor and DataFence. Migration/startup
    // subsequently receive this very holder, including newly created targets.
    for spec in profile.targets() {
        let path = root.path().join(spec.path.trim_start_matches('/'));
        let retained = same
            ._root_files
            .iter()
            .find(|f| f.observed.path == path)
            .expect("fixed Root descriptor retained");
        retained.revalidate().unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            retained._file.metadata().unwrap().ino(),
            fs::metadata(path).unwrap().ino()
        );
    }
    assert!(rustsimhub_storage::Store::open(&data).is_err());
    drop(same);
    assert!(rustsimhub_storage::Store::open(&data).is_ok());
}
#[test]
fn unrelated_root_change_invalidates_permanently_without_releasing_original_writer() {
    let root = crate::admission::tests::fixture(Mode::Forwarding, true);
    let bundle = support::Bundle::new(Profile::Legacy);
    prepare(root.path(), &bundle);
    let authority = Authority {
        tx: bundle.verify().unwrap().manifest_sha256().into(),
        profile: Profile::Legacy,
        calls: Cell::new(0),
    };
    let fence = DataFence::at(
        crate::admission::tests::snapshot(root.path()),
        &authority,
        root.path(),
    )
    .unwrap();
    let services = Rc::new(IsolatedServiceState {
        off: Cell::new(true),
        ..Default::default()
    });
    let mut installer = ProductInstaller::prepare_isolated(
        root.path(),
        &bundle.public(),
        &bundle.bindings,
        &fence,
        services,
        false,
    )
    .unwrap();
    put(
        root.path(),
        "/etc/not-owned-by-install.conf",
        b"foreign",
        0o644,
    );
    assert!(installer.apply().is_err());
    fs::remove_file(root.path().join("etc/not-owned-by-install.conf")).unwrap();
    assert!(installer.inspect().is_err());
    assert!(rustsimhub_storage::Store::open(root.path().join(STATE)).is_err());
    assert!(fence.into_snapshot().is_err());
}
