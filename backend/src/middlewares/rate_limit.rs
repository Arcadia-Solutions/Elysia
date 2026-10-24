use std::collections::HashMap;
use std::hash::Hash;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use actix_web::body::{EitherBody, MessageBody};
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{HttpRequest, web::Data};
use governor::clock::Clock;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use ipnet::{IpNet, Ipv6Net};
use uuid::Uuid;

use crate::config::RateLimitConfig;
use crate::error::{Error, Result};

// we use `governor` directly rather than `actix-governor`: the per-user limit runs in
// the auth extractors, once the token is resolved and before the user lookup,
// which a middleware could only do by authenticating twice; and the 429 goes
// through `Error` like every other API error (JSON body, Retry-After,
// telemetry), where `actix-governor` builds its own response.

/// The keyed limiters, each None when its limit is 0 or the `rate_limit`
/// config is absent. Without this in `app_data` (most tests), nothing is
/// limited.
// ponytail: in-memory and per process, move the buckets to Postgres or Redis if
// Elysia ever runs several replicas.
pub struct RateLimits {
    unauthenticated_per_ip: Option<DefaultKeyedRateLimiter<IpAddr>>,
    login_per_ip: Option<DefaultKeyedRateLimiter<IpAddr>>,
    authenticated_per_user: Option<DefaultKeyedRateLimiter<Uuid>>,
    trusted_proxies: Vec<IpNet>,
    /// IPs whose failed authentications ran out their unauthenticated
    /// budget, refused until then without their credentials being checked.
    auth_blocked_until: Mutex<HashMap<IpAddr, Instant>>,
}

fn limiter<K: Hash + Eq + Clone>(per_minute: u32) -> Option<DefaultKeyedRateLimiter<K>> {
    NonZeroU32::new(per_minute).map(|per_minute| RateLimiter::keyed(Quota::per_minute(per_minute)))
}

/// Take one request from `key`'s bucket, or fail with the seconds until one refills.
fn check<K: Hash + Eq + Clone>(
    limiter: &Option<DefaultKeyedRateLimiter<K>>,
    key: &K,
) -> Result<()> {
    let Some(limiter) = limiter else {
        return Ok(());
    };
    limiter.check_key(key).map_err(|not_until| {
        let wait = not_until.wait_time_from(limiter.clock().now());
        Error::TooManyRequests(wait.as_secs() + 1)
    })
}

fn forget_idle<K: Hash + Eq + Clone>(limiter: &Option<DefaultKeyedRateLimiter<K>>) {
    if let Some(limiter) = limiter {
        limiter.retain_recent();
        limiter.shrink_to_fit();
    }
}

impl RateLimits {
    pub fn new(config: Option<&RateLimitConfig>) -> Self {
        Self {
            unauthenticated_per_ip: limiter(config.map_or(0, |config| {
                config.unauthenticated_requests_per_minute_per_ip
            })),
            login_per_ip: limiter(
                config.map_or(0, |config| config.login_attempts_per_minute_per_ip),
            ),
            authenticated_per_user: limiter(config.map_or(0, |config| {
                config.authenticated_requests_per_minute_per_user
            })),
            trusted_proxies: config.map_or_else(Vec::new, |config| config.trusted_proxies.clone()),
            auth_blocked_until: Mutex::default(),
        }
    }

    pub fn check_user(&self, user_id: Uuid) -> Result<()> {
        check(&self.authenticated_per_user, &user_id)
    }

    /// Drop the keys whose bucket is full again, so memory follows the active
    /// clients. Run periodically by the scheduler.
    pub fn forget_idle(&self) {
        forget_idle(&self.unauthenticated_per_ip);
        forget_idle(&self.login_per_ip);
        forget_idle(&self.authenticated_per_user);
        let now = Instant::now();
        self.auth_blocked_until
            .lock()
            .expect("rate limit lock poisoned")
            .retain(|_, until| *until > now);
    }

    fn is_trusted_proxy(&self, ip: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|range| range.contains(&ip))
    }

    /// The peer, or behind a trusted proxy the rightmost `X-Forwarded-For`
    /// entry that is not a trusted proxy: entries left of it are client
    /// supplied and forgeable. When every entry is trusted (a client on the
    /// proxy's private network), the leftmost one, so LAN clients do not all
    /// share the proxy's bucket. IPv6 is keyed by /64, the smallest block a
    /// client usually gets, so it cannot rotate addresses inside it.
    fn client_ip(&self, request: &HttpRequest) -> Option<IpAddr> {
        let peer = request.peer_addr()?.ip().to_canonical();
        let client = if self.is_trusted_proxy(peer) {
            let mut leftmost = None;
            request
                .headers()
                .get("X-Forwarded-For")
                .and_then(|header| header.to_str().ok())
                .unwrap_or_default()
                .rsplit(',')
                .filter_map(|entry| entry.trim().parse::<IpAddr>().ok())
                .map(|ip| ip.to_canonical())
                .inspect(|ip| leftmost = Some(*ip))
                .find(|ip| !self.is_trusted_proxy(*ip))
                .or(leftmost)
                .unwrap_or(peer)
        } else {
            peer
        };
        Some(match client {
            IpAddr::V6(ip) => IpAddr::V6(Ipv6Net::new_assert(ip, 64).network()),
            ip => ip,
        })
    }

    /// Check the client IP against one of the per-IP limiters.
    fn check_ip(
        &self,
        request: &HttpRequest,
        limiter: &Option<DefaultKeyedRateLimiter<IpAddr>>,
    ) -> Result<()> {
        match self.client_ip(request) {
            Some(ip) => check(limiter, &ip),
            None => Ok(()),
        }
    }
}

/// Failed authentications of one request's client IP. A failure spends the
/// IP's unauthenticated budget; once that runs out the IP is refused before
/// its credentials are even checked, so a right guess cannot slip through
/// and the guesses stop costing queries. Successes cost nothing.
pub struct FailedAuthLimit(Option<(Data<RateLimits>, IpAddr)>);

impl FailedAuthLimit {
    /// Inert without `RateLimits` in `app_data` or without a peer address.
    pub fn new(request: &HttpRequest) -> Self {
        Self(
            request
                .app_data::<Data<RateLimits>>()
                .and_then(|limits| Some((limits.clone(), limits.client_ip(request)?))),
        )
    }

    /// Fail while the IP is blocked. Call it before checking credentials that
    /// cost a query or a secret comparison.
    pub fn check_blocked(&self) -> Result<()> {
        let Some((limits, ip)) = &self.0 else {
            return Ok(());
        };
        let blocked = limits
            .auth_blocked_until
            .lock()
            .expect("rate limit lock poisoned");
        match blocked
            .get(ip)
            .and_then(|until| until.checked_duration_since(Instant::now()))
        {
            Some(wait) => Err(Error::TooManyRequests(wait.as_secs() + 1)),
            None => Ok(()),
        }
    }

    /// Charge an `Unauthorized` result to the IP's unauthenticated budget,
    /// turning it into a 429 and blocking the IP when the budget is out.
    pub fn charge<T>(&self, result: Result<T>) -> Result<T> {
        let (Some((limits, ip)), Err(Error::Unauthorized)) = (&self.0, &result) else {
            return result;
        };
        match check(&limits.unauthenticated_per_ip, ip) {
            Err(Error::TooManyRequests(seconds)) => {
                limits
                    .auth_blocked_until
                    .lock()
                    .expect("rate limit lock poisoned")
                    .insert(*ip, Instant::now() + Duration::from_secs(seconds));
                Err(Error::TooManyRequests(seconds))
            }
            _ => result,
        }
    }
}

/// Answer 429 when `pick`'s limiter refuses the client IP, else run the
/// request. The 429 is a response rather than an error, so the outer
/// middlewares (CORS, tracing) still process it.
async fn limit_with<B: MessageBody>(
    request: ServiceRequest,
    next: Next<B>,
    pick: fn(&RateLimits) -> &Option<DefaultKeyedRateLimiter<IpAddr>>,
) -> std::result::Result<ServiceResponse<EitherBody<B>>, actix_web::Error> {
    let checked = match request.app_data::<Data<RateLimits>>() {
        Some(limits) => limits.check_ip(request.request(), pick(limits)),
        None => Ok(()),
    };
    if let Err(error) = checked {
        return Ok(request.error_response(error).map_into_right_body());
    }
    next.call(request)
        .await
        .map(ServiceResponse::map_into_left_body)
}

/// Middleware limiting the routes that need no login, per client IP. Wrapped
/// on each such route, so authenticated routes stay out of it.
pub async fn limit_unauthenticated(
    request: ServiceRequest,
    next: Next<impl MessageBody>,
) -> std::result::Result<ServiceResponse<EitherBody<impl MessageBody>>, actix_web::Error> {
    limit_with(request, next, |limits| &limits.unauthenticated_per_ip).await
}

/// Middleware limiting login, registration and password change attempts per
/// client IP.
pub async fn limit_login(
    request: ServiceRequest,
    next: Next<impl MessageBody>,
) -> std::result::Result<ServiceResponse<EitherBody<impl MessageBody>>, actix_web::Error> {
    limit_with(request, next, |limits| &limits.login_per_ip).await
}
