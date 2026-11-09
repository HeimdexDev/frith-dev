use frith_release::*;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let checker = LeanChecker::new(demo::checker_path(), Duration::from_secs(2))?;
    let (manifest, context, receipt) = demo::fixture();
    let operator = Actor {
        tenant: manifest.tenant.clone(),
        role: Role::Operator,
    };
    let security = Actor {
        tenant: manifest.tenant.clone(),
        role: Role::SecurityOfficer,
    };
    let mut coordinator = Coordinator::default();
    let key = coordinator.register(&operator, manifest)?;
    for (revision, action) in [
        Action::Authorize,
        Action::Complete,
        Action::Publish,
        Action::Revoke,
    ]
    .into_iter()
    .enumerate()
    {
        let actor = if action == Action::Revoke {
            &security
        } else {
            &operator
        };
        let result = coordinator.apply(
            &checker,
            actor,
            &key,
            Command {
                action,
                expected_revision: revision as u64,
                receipt: if action == Action::Complete {
                    Some(receipt.clone())
                } else {
                    None
                },
            },
            &context,
        )?;
        println!("{:?} -> revision {}", result.stage, result.revision);
    }
    let late = coordinator.apply(
        &checker,
        &operator,
        &key,
        Command {
            action: Action::Publish,
            expected_revision: 4,
            receipt: None,
        },
        &context,
    );
    assert_eq!(late, Err(Error::Denied));
    println!("Late publication after revocation: denied");
    Ok(())
}
