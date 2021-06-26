use crate::common::*;
use parameterized_macro::parameterized;

#[parameterized(env_name = {"default", "alpine"})]
fn test_query(env_name: &str) {
    let state = setup();
    if !cfg!(feature = "docker") && env_name == "alpine" {
        return;
    }

    state.rt.block_on(async {
        publish_components(&state, &env_name, vec!["heylib", "helloworld"], "1")
            .await
            .expect("published heylib=1 helloworld=1");

        let r = lal::query(&state.backend, Some(&env_name), "hello", false).await;
        assert!(r.is_ok(), "could query for hello");
    });
}

#[parameterized(env_name = {"default", "alpine"})]
fn test_query_last(env_name: &str) {
    let state = setup();
    if !cfg!(feature = "docker") && env_name == "alpine" {
        return;
    }

    state.rt.block_on(async {
        publish_components(&state, &env_name, vec!["heylib", "helloworld"], "1")
            .await
            .expect("published heylib=1 helloworld=1");

        let r = lal::query(&state.backend, Some(&env_name), "hello", true).await;
        assert!(r.is_ok(), "could query for hello");
    });
}
