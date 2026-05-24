use playwright::Playwright;
use playwright::api::route::UrlMatcher;
use std::sync::Arc;
use playwright::api::{route::Route, Request};

playwright::runtime_test!(test_page_route, {
    main().await.unwrap();
});

async fn main() -> Result<(), playwright::Error> {
    let playwright = Playwright::initialize().await?;
    let chromium = playwright.chromium();
    let browser = chromium.launcher().headless(true).launch().await?;
    let context = browser.context_builder().build().await?;
    let page = context.new_page().await?;

    let matcher = UrlMatcher::new_glob("**/*").unwrap();
    
    // Setup a route that aborts all requests
    page.route(
        matcher.clone(),
        Arc::new(|route: Route, _req: Request| {
            Box::pin(async move {
                route.abort(None).await.unwrap();
            })
        }),
    ).await.unwrap();

    // Try navigating. It should fail since we abort everything.
    let res = page.goto_builder("https://example.com").goto().await;
    assert!(res.is_err()); // Navigation should fail due to abort

    // Unroute it
    page.unroute(&matcher).await.unwrap();

    // Now it should succeed
    let res2 = page.goto_builder("https://example.com").goto().await;
    assert!(res2.is_ok());

    Ok(())
}
