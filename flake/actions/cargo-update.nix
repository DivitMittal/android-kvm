{
  common-permissions,
  common-actions,
  ...
}: {
  flake.actions-nix.workflows.".github/workflows/cargo-update.yml" = {
    on = {
      workflow_dispatch = {};
      schedule = [
        {
          cron = "0 0 1-7 * 0"; # First Sunday of every month at midnight
        }
      ];
    };
    jobs.updating-cargo = {
      permissions =
        common-permissions
        // {
          pull-requests = "write";
        };
      steps =
        common-actions
        ++ [
          {
            name = "Update Cargo dependencies";
            run = "nix develop --command cargo update";
          }
          {
            name = "Create Pull Request";
            uses = "peter-evans/create-pull-request@v7";
            "with" = {
              commit-message = "chore(deps): update Cargo dependencies";
              title = "chore(deps): update Cargo dependencies";
              body = "Automated Cargo dependency update.";
              branch = "cargo-update";
            };
          }
        ];
    };
  };
}
