#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking and reads the document it rendered by key"
)]

use std::ffi::OsString;
use std::net::Ipv4Addr;

use afr_engine::Limits;
use serde_json::{Value, json};

use super::{Boot, GuestNetwork, render};

#[test]
fn test_the_config_file_names_every_drive_its_cache_its_rate_and_the_vsock() {
    let granted = crate::grant(Limits {
        cpu_millis: 1_500,
        memory_bytes: 2 * 1024 * 1024 * 1024,
        ..Limits::DEFAULT
    })
    .unwrap();
    let entry = [
        OsString::from("guest"),
        OsString::from("--max-read-bytes"),
        OsString::from("7"),
    ];
    let networked = Boot {
        granted: &granted,
        entry: &entry,
        io_bytes_per_second: 209_715_200,
        network: Some(GuestNetwork {
            address: Ipv4Addr::new(172, 16, 0, 2),
            gateway: Ipv4Addr::new(172, 16, 0, 1),
            prefix_len: 30,
        }),
        names: true,
    };

    let document: Value = serde_json::from_str(&render(&networked, "tap0").unwrap()).unwrap();
    let isolated: Value = serde_json::from_str(
        &render(
            &Boot {
                network: None,
                names: false,
                ..networked
            },
            "tap0",
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        document["drives"],
        json!([
            {"drive_id": "toolbox", "path_on_host": "/toolbox", "is_root_device": false, "is_read_only": true},
            {"drive_id": "workspace", "path_on_host": "/workspace.img", "is_root_device": false,
             "is_read_only": false, "cache_type": "Writeback",
             "rate_limiter": {"bandwidth": {"size": 209_715_200, "refill_time": 1000}}},
            {"drive_id": "names", "path_on_host": "/names", "is_root_device": false, "is_read_only": true},
        ])
    );
    assert_eq!(
        document["vsock"],
        json!({"guest_cid": 3, "uds_path": "/v.sock"})
    );
    assert_eq!(
        document["machine-config"],
        json!({"vcpu_count": 2, "mem_size_mib": 2048})
    );
    assert_eq!(
        document["network-interfaces"],
        json!([{"iface_id": "eth0", "host_dev_name": "tap0"}])
    );
    assert_eq!(
        document["boot-source"],
        json!({
            "kernel_image_path": "/vmlinux",
            "initrd_path": "/initrd",
            "boot_args": "console=ttyS0 quiet reboot=k panic=1 pci=off \
                          ip=172.16.0.2::172.16.0.1:255.255.255.252::eth0:off -- guest --max-read-bytes 7",
        })
    );
    assert_eq!(isolated["network-interfaces"], json!([]));
    assert_eq!(isolated["drives"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        isolated["boot-source"]["boot_args"],
        "console=ttyS0 quiet reboot=k panic=1 pci=off -- guest --max-read-bytes 7"
    );
}
