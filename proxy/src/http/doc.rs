use warp::{document, path, reply, Filter, Rejection, Reply};

pub fn filters<F: Filter>(
    routes: &F,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    index_filter().or(describe_filter(routes))
}

/// GET /
/// GET /index.html
pub fn index_filter() -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    path::end().or(path("index.html")).map(|_| {
        warp::http::Response::builder()
            .header("content-type", "text/html; charset=utf-8")
            .body(INDEX_HTML)
    })
}

/// GET /opnapi.json
pub fn describe_filter<F: Filter>(
    routes: &F,
) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
    let docs = document::to_openapi(document::describe(routes));

    path!("openapi.json").map(move || reply::json(&docs))
}

const INDEX_HTML: &str = r#"
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <script type="module" src="https://unpkg.com/rapidoc/dist/rapidoc-min.js"></script>
</head>
<body>
  <rapi-doc 
    spec-url = "/docs/openapi.json"
    render-style = "read"
    defualt-schema-tab = "model"
    schema-style = "table"
    >
  </rapi-doc>
</body> 
</html>
"#;
