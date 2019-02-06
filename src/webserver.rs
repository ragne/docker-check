
use warp::{Filter, Server, Reply, Rejection, filters::BoxedFilter};
use warp::http::StatusCode;
use std::error::Error as StdError;
use crate::docker_checker::Stats;

pub(crate) struct HealthCheckServer {
  addr: [u8;4],
  stats: Stats,
  port: u16
}

#[derive(Copy, Clone, Debug)]
enum Error {
    Oops,
    Nope,
}

#[derive(Serialize)]
struct ErrorMessage {
    code: u16,
    message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match self {
            Error::Oops => ":fire: this is fine",
            Error::Nope => "Nope!",
        }
    }

    fn cause(&self) -> Option<&std::error::Error> {
        None
    }
}

fn customize_error(err: Rejection) -> Result<impl Reply, Rejection> {
    if let Some(&err) = err.find_cause::<Error>() {
        let code = match err {
            Error::Nope => StatusCode::BAD_REQUEST,
            Error::Oops => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let msg = err.to_string();

        let json = warp::reply::json(&ErrorMessage {
            code: code.as_u16(),
            message: msg,
        });
        Ok(warp::reply::with_status(json, code))
    } else {
        // Could be a NOT_FOUND, or METHOD_NOT_ALLOWED... here we just
        // let warp use its default rendering.
        Err(err)
    }
}

impl HealthCheckServer {
  pub fn new(addr: [u8;4], port:u16, stats: Stats) -> Self {
    Self {
     stats,
     addr,
     port
    }
  }

  pub fn serve(self) -> () {
    let (routes, addr, port) = self.routes();
    warp::serve(routes).run((addr, port))
  }

  fn routes(self) -> (BoxedFilter<(impl Reply,)>,[u8;4],u16) {
    let hello = warp::path::end().map(warp::reply);
    let cloned = self.stats.clone();
    let health = warp::path("health").map(move || warp::reply::json(&ErrorMessage {
            code: 200,
            message: format!("{:?}", cloned),
        }));
    let oops =
        warp::path("oops").and_then(|| Err::<StatusCode, _>(warp::reject::custom(Error::Oops)));

    let nope =
        warp::path("nope").and_then(|| Err::<StatusCode, _>(warp::reject::custom(Error::Nope)));

    let routes = warp::get2()
        .and(hello.or(oops).or(nope).or(health))
        .recover(customize_error);
    (routes.boxed(), self.addr, self.port)
  }
}
