// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::LazyLock,
    time::Duration,
};

use ipnet::IpNet;
use reqwest::{
    Client, Url,
    dns::{Addrs, Name, Resolve, Resolving},
    redirect::Policy,
};

use crate::config::OutboundAddressPolicy;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const NON_PUBLIC_NETWORK_CIDRS: &[&str] = &[
    "0.0.0.0/8",
    "10.0.0.0/8",
    "100.64.0.0/10",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.0.0.0/24",
    "192.0.2.0/24",
    "192.88.99.0/24",
    "192.168.0.0/16",
    "198.18.0.0/15",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "::/128",
    "::1/128",
    "64:ff9b::/96",
    "64:ff9b:1::/48",
    "100::/64",
    "100:0:0:1::/64",
    "2001::/23",
    "2001:db8::/32",
    "2002::/16",
    "3fff::/20",
    "5f00::/16",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
];

static NON_PUBLIC_NETWORKS: LazyLock<Vec<IpNet>> = LazyLock::new(|| {
    NON_PUBLIC_NETWORK_CIDRS
        .iter()
        .map(|cidr| cidr.parse().expect("hardcoded network CIDR is valid"))
        .collect()
});

pub(crate) fn build_client(policy: OutboundAddressPolicy) -> Result<Client, reqwest::Error> {
    Client::builder()
        .dns_resolver(PublicDnsResolver { policy })
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
}

pub(crate) fn validate_literal_host(
    url: &Url,
    policy: OutboundAddressPolicy,
) -> Result<(), IpAddr> {
    if policy == OutboundAddressPolicy::AllowPrivateAddress {
        return Ok(());
    }

    match url.host() {
        Some(url::Host::Ipv4(address)) => validate_public_address(address.into()),
        Some(url::Host::Ipv6(address)) => validate_public_address(address.into()),
        Some(url::Host::Domain(_)) | None => Ok(()),
    }
}

#[derive(Clone, Copy, Debug)]
struct PublicDnsResolver {
    policy: OutboundAddressPolicy,
}

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_string();
        let policy = self.policy;

        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .collect::<Vec<SocketAddr>>();
            if addresses.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{host} resolved to no addresses"),
                )
                .into());
            }
            if policy == OutboundAddressPolicy::PublicOnly {
                for address in &addresses {
                    validate_public_address(address.ip()).map_err(|blocked| {
                        io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            format!("{host} resolved to non-public address {blocked}"),
                        )
                    })?;
                }
            }

            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn validate_public_address(address: IpAddr) -> Result<(), IpAddr> {
    if let IpAddr::V6(address) = address
        && let Some(mapped) = address.to_ipv4_mapped()
    {
        return validate_public_address(mapped.into());
    }
    let is_public = !NON_PUBLIC_NETWORKS
        .iter()
        .any(|network| network.contains(&address));

    if is_public { Ok(()) } else { Err(address) }
}
