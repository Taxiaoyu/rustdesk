use hbb_common::{anyhow::anyhow, bail, log, ResultType};
use std::{
    collections::{HashMap, HashSet},
    mem,
    net::Ipv4Addr,
    ptr,
    slice,
    sync::Mutex,
};
use winapi::{
    shared::{
        ipmib::{MIB_IPFORWARDROW, MIB_IPFORWARDTABLE, MIB_IPROUTE_TYPE_INDIRECT},
        minwindef::FALSE,
        nldef::PROTO_IP_NETMGMT,
        winerror::{ERROR_INSUFFICIENT_BUFFER, ERROR_OBJECT_ALREADY_EXISTS, NO_ERROR},
    },
    um::{
        handleapi::CloseHandle,
        iphlpapi::{CreateIpForwardEntry, DeleteIpForwardEntry, GetIpForwardTable},
        minwinbase::STILL_ACTIVE,
        processthreadsapi::{GetExitCodeProcess, OpenProcess},
        winnt::PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

const TUN_BYPASS_ROUTE_METRIC: u32 = 4_242;

#[derive(Clone, Copy)]
struct ManagedRoute {
    row: MIB_IPFORWARDROW,
    references: usize,
    created_by_us: bool,
}

#[derive(Default)]
struct RouteLease {
    owner_pid: u32,
    addresses: HashSet<Ipv4Addr>,
}

#[derive(Default)]
struct RouteManager {
    leases: HashMap<String, RouteLease>,
    routes: HashMap<Ipv4Addr, ManagedRoute>,
}

lazy_static::lazy_static! {
    static ref ROUTE_MANAGER: Mutex<RouteManager> = Mutex::new(RouteManager::default());
}

fn windows_ipv4(address: Ipv4Addr) -> u32 {
    u32::from_ne_bytes(address.octets())
}

fn ipv4_from_windows(address: u32) -> Ipv4Addr {
    Ipv4Addr::from(address.to_ne_bytes())
}

fn is_benchmark_address(address: Ipv4Addr) -> bool {
    let [a, b, _, _] = address.octets();
    a == 198 && (18..=19).contains(&b)
}

fn is_public_address(address: Ipv4Addr) -> bool {
    let [a, b, _, _] = address.octets();
    !address.is_private()
        && !address.is_loopback()
        && !address.is_link_local()
        && !address.is_unspecified()
        && !address.is_broadcast()
        && !address.is_multicast()
        && !(a == 100 && (64..=127).contains(&b))
        && !is_benchmark_address(address)
        && !(a == 192 && b == 0)
}

fn get_ipv4_routes() -> ResultType<Vec<MIB_IPFORWARDROW>> {
    let mut size = 0u32;
    let first_result = unsafe { GetIpForwardTable(ptr::null_mut(), &mut size, FALSE) };
    if first_result != ERROR_INSUFFICIENT_BUFFER || size == 0 {
        bail!("GetIpForwardTable(size) failed with error {first_result}");
    }

    let word_size = mem::size_of::<usize>();
    let word_count = (size as usize + word_size - 1) / word_size;
    let mut storage = vec![0usize; word_count];
    let table = storage.as_mut_ptr() as *mut MIB_IPFORWARDTABLE;
    let result = unsafe { GetIpForwardTable(table, &mut size, FALSE) };
    if result != NO_ERROR {
        bail!("GetIpForwardTable failed with error {result}");
    }

    let count = unsafe { (*table).dwNumEntries as usize };
    let first = unsafe { (*table).table.as_ptr() };
    Ok(unsafe { slice::from_raw_parts(first, count) }.to_vec())
}

fn find_physical_default_route() -> ResultType<MIB_IPFORWARDROW> {
    get_ipv4_routes()?
        .into_iter()
        .filter(|row| {
            row.dwForwardDest == 0
                && row.dwForwardMask == 0
                && row.dwForwardNextHop != 0
                && row.dwForwardIfIndex != 1
                && row.ForwardType == MIB_IPROUTE_TYPE_INDIRECT
        })
        .min_by_key(|row| row.dwForwardMetric1)
        .ok_or_else(|| anyhow!("No usable physical IPv4 default route was found"))
}

pub(crate) fn is_clash_tun_active() -> bool {
    get_ipv4_routes().map_or(false, |routes| {
        routes.into_iter().any(|route| {
            route.dwForwardNextHop != 0
                && is_benchmark_address(ipv4_from_windows(route.dwForwardNextHop))
        })
    })
}

fn create_host_route(address: Ipv4Addr) -> ResultType<ManagedRoute> {
    let mut row = find_physical_default_route()?;
    row.dwForwardDest = windows_ipv4(address);
    row.dwForwardMask = u32::MAX;
    row.dwForwardPolicy = 0;
    row.ForwardType = MIB_IPROUTE_TYPE_INDIRECT;
    row.ForwardProto = PROTO_IP_NETMGMT;
    row.dwForwardAge = 0;
    row.dwForwardNextHopAS = 0;
    row.dwForwardMetric1 = TUN_BYPASS_ROUTE_METRIC;
    row.dwForwardMetric2 = u32::MAX;
    row.dwForwardMetric3 = u32::MAX;
    row.dwForwardMetric4 = u32::MAX;
    row.dwForwardMetric5 = u32::MAX;

    let existing_route = get_ipv4_routes()?.into_iter().find(|existing| {
        existing.dwForwardDest == row.dwForwardDest
            && existing.dwForwardMask == row.dwForwardMask
            && existing.dwForwardNextHop == row.dwForwardNextHop
            && existing.dwForwardIfIndex == row.dwForwardIfIndex
            && existing.ForwardProto == PROTO_IP_NETMGMT
            && existing.dwForwardMetric1 == TUN_BYPASS_ROUTE_METRIC
    });
    let (row, created_by_us) = if let Some(existing_route) = existing_route {
        (existing_route, true)
    } else {
        let result = unsafe { CreateIpForwardEntry(&mut row) };
        match result {
            NO_ERROR => (row, true),
            ERROR_OBJECT_ALREADY_EXISTS => (row, false),
            _ => bail!(
                "CreateIpForwardEntry failed for {address} via {} (interface {}), error {result}",
                ipv4_from_windows(row.dwForwardNextHop),
                row.dwForwardIfIndex
            ),
        }
    };
    log::info!(
        "TUN bypass route ready: {address}/32 via {} (interface {}, created={created_by_us})",
        ipv4_from_windows(row.dwForwardNextHop),
        row.dwForwardIfIndex
    );
    Ok(ManagedRoute {
        row,
        references: 1,
        created_by_us,
    })
}

fn delete_route(address: Ipv4Addr, route: ManagedRoute) {
    if !route.created_by_us {
        return;
    }
    let mut row = route.row;
    let result = unsafe { DeleteIpForwardEntry(&mut row) };
    if result == NO_ERROR {
        log::info!("Removed TUN bypass route: {address}/32");
    } else {
        log::warn!("DeleteIpForwardEntry failed for {address}, error {result}");
    }
}

pub(crate) fn add_tun_bypass_route(
    lease_id: &str,
    owner_pid: u32,
    address: Ipv4Addr,
) -> ResultType<()> {
    if lease_id.len() != 36 || owner_pid == 0 {
        bail!("Invalid TUN bypass route lease");
    }
    if !is_public_address(address) {
        bail!("Refused non-public TUN bypass address {address}");
    }

    let mut manager = ROUTE_MANAGER.lock().unwrap();
    if let Some(lease) = manager.leases.get(lease_id) {
        if lease.owner_pid != owner_pid {
            bail!("TUN bypass route lease owner mismatch");
        }
        if lease.addresses.contains(&address) {
            return Ok(());
        }
    }

    if let Some(route) = manager.routes.get_mut(&address) {
        route.references += 1;
    } else {
        let route = create_host_route(address)?;
        manager.routes.insert(address, route);
    }
    manager
        .leases
        .entry(lease_id.to_owned())
        .or_insert_with(|| RouteLease {
            owner_pid,
            addresses: HashSet::new(),
        })
        .addresses
        .insert(address);
    Ok(())
}

pub(crate) fn remove_tun_bypass_routes(lease_id: &str, owner_pid: u32) -> ResultType<()> {
    let mut manager = ROUTE_MANAGER.lock().unwrap();
    let Some(lease) = manager.leases.remove(lease_id) else {
        return Ok(());
    };
    if owner_pid != 0 && lease.owner_pid != owner_pid {
        manager.leases.insert(lease_id.to_owned(), lease);
        bail!("TUN bypass route lease owner mismatch");
    }
    for address in lease.addresses {
        let Some(route) = manager.routes.get_mut(&address) else {
            continue;
        };
        route.references = route.references.saturating_sub(1);
        if route.references == 0 {
            if let Some(route) = manager.routes.remove(&address) {
                delete_route(address, route);
            }
        }
    }
    Ok(())
}

fn process_is_running(pid: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0;
    let running = unsafe { GetExitCodeProcess(handle, &mut exit_code) != FALSE }
        && exit_code == STILL_ACTIVE;
    unsafe {
        CloseHandle(handle);
    }
    running
}

pub(crate) fn cleanup_tun_bypass_routes() {
    let stale: Vec<(String, u32)> = {
        let manager = ROUTE_MANAGER.lock().unwrap();
        manager
            .leases
            .iter()
            .filter_map(|(id, lease)| {
                (!process_is_running(lease.owner_pid)).then(|| (id.clone(), lease.owner_pid))
            })
            .collect()
    };
    for (id, pid) in stale {
        if let Err(err) = remove_tun_bypass_routes(&id, pid) {
            log::warn!("Failed to clean stale TUN bypass route lease {id}: {err}");
        }
    }
}

pub(crate) fn clear_tun_bypass_routes() {
    let leases: Vec<(String, u32)> = {
        let manager = ROUTE_MANAGER.lock().unwrap();
        manager
            .leases
            .iter()
            .map(|(id, lease)| (id.clone(), lease.owner_pid))
            .collect()
    };
    for (id, pid) in leases {
        if let Err(err) = remove_tun_bypass_routes(&id, pid) {
            log::warn!("Failed to clear TUN bypass route lease {id}: {err}");
        }
    }
}

pub(crate) fn scrub_stale_tun_bypass_routes() {
    let Ok(routes) = get_ipv4_routes() else {
        return;
    };
    for mut row in routes.into_iter().filter(|row| {
        row.dwForwardMask == u32::MAX
            && row.ForwardProto == PROTO_IP_NETMGMT
            && row.dwForwardMetric1 == TUN_BYPASS_ROUTE_METRIC
    }) {
        let address = ipv4_from_windows(row.dwForwardDest);
        let result = unsafe { DeleteIpForwardEntry(&mut row) };
        if result == NO_ERROR {
            log::info!("Removed stale TUN bypass route: {address}/32");
        } else {
            log::warn!("Failed to remove stale TUN bypass route {address}, error {result}");
        }
    }
}
