
use warp::{Filter, Reply, filters::BoxedFilter};
use crate::docker_checker::Stats;

pub(crate) struct HealthCheckServer {
  addr: [u8;4],
  stats: Stats,
  port: u16
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
    let cloned = self.stats.clone();
    let health = warp::path("health").map(move || warp::reply::json(
        &*cloned.read().unwrap()
        ));

    let routes = warp::get2()
        .and(health);
    (routes.boxed(), self.addr, self.port)
  }
}
