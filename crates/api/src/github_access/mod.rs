mod cached;
mod error_conversion;
mod installation_sync;
mod real;
mod test_doubles;
mod types;

// 外部公開API — 既存の `use boardflow_api::github_access::*` が壊れないように全て再エクスポート
pub use cached::CachedGithubAccessChecker;
pub(crate) use error_conversion::access_error_to_app_error;
pub use error_conversion::access_result_to_error;
pub use real::RealGithubAccessChecker;
pub use test_doubles::{
    AllowAllGithubAccessChecker, DenyAllGithubAccessChecker, RateLimitedGithubAccessChecker,
    TokenExpiredGithubAccessChecker, UpstreamErrorGithubAccessChecker,
};
pub use types::{AccessError, AccessResult, DynGithubAccessChecker, GithubAccessChecker};
