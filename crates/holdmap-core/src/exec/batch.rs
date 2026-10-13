//! Reuse only fully successful, pinned supervisor effects within one reviewed batch.

use crate::{ActionPlan, Step, StopReport};

/// In-memory completed supervisor commands. Callers must preflight every original plan
/// before execution, keep each target's verification and record history from its original
/// plan. A caller with persistence must remember an effect only after its history is saved.
#[derive(Default)]
pub struct CompletedSupervisorStops {
    commands: Vec<Step>,
}

impl CompletedSupervisorStops {
    /// Execution copy with exact commands completed earlier in this batch omitted.
    /// Process signals and every listener verification retain their original guards.
    pub fn remaining(&self, plan: &ActionPlan) -> ActionPlan {
        let mut execution = plan.clone();
        execution
            .steps
            .retain(|step| !self.commands.iter().any(|done| same_command(step, done)));
        execution
    }

    /// Remember only a successful effect whose listener verification also completed.
    /// Failed or partially completed plans cannot authorize reuse of a command.
    pub fn remember(&mut self, execution: &ActionPlan, report: &StopReport) {
        if !report.success || !report.freed {
            return;
        }
        for step in &execution.steps {
            if matches!(step, Step::RunCommand { guard: Some(_), .. })
                && !self.commands.iter().any(|done| same_command(step, done))
            {
                self.commands.push(step.clone());
            }
        }
    }
}

/// Display text cannot change command identity. Only an exact program/argument pair with
/// the same pinned guard can reuse a successful effect; every VerifyFree step is retained.
fn same_command(left: &Step, right: &Step) -> bool {
    match (left, right) {
        (
            Step::RunCommand {
                program: a,
                args: aa,
                guard: Some(ag),
                ..
            },
            Step::RunCommand {
                program: b,
                args: ba,
                guard: Some(bg),
                ..
            },
        ) => a == b && aa == ba && ag.pid == bg.pid && ag.start_token == bg.start_token,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcRef;

    fn command(pid: u32, token: u64, program: &str, id: &str, reason: &str) -> Step {
        Step::RunCommand {
            program: program.into(),
            args: vec!["stop".into(), id.into()],
            reason: reason.into(),
            guard: Some(ProcRef {
                pid,
                start_token: token,
                name: "node".into(),
                command: "node server.js".into(),
            }),
        }
    }

    #[test]
    fn supervisor_reuse_requires_identical_execution_arguments_and_pinned_identity() {
        let original = command(101, 10, "pm2", "0", "first port");
        assert!(same_command(
            &original,
            &command(101, 10, "pm2", "0", "second port")
        ));
        for changed in [
            command(102, 10, "pm2", "0", "second port"),
            command(101, 11, "pm2", "0", "second port"),
            command(101, 10, "systemctl", "0", "second port"),
            command(101, 10, "pm2", "1", "second port"),
        ] {
            assert!(!same_command(&original, &changed));
        }
        let mut unpinned = original.clone();
        if let Step::RunCommand { guard, .. } = &mut unpinned {
            *guard = None;
        }
        assert!(!same_command(&unpinned, &unpinned));
        let verify = Step::VerifyFree {
            port: 3000,
            protocol: crate::Protocol::Tcp,
            timeout_ms: 3000,
        };
        assert!(!same_command(&verify, &verify));
    }

    #[test]
    fn only_fully_completed_reports_can_remove_commands_from_later_execution_copies() {
        let original = ActionPlan {
            target: "3000".into(),
            owners: vec![],
            summary: "fixture supervisor".into(),
            steps: vec![
                command(101, 10, "pm2", "0", "first port"),
                Step::VerifyFree {
                    port: 3000,
                    protocol: crate::Protocol::Tcp,
                    timeout_ms: 3000,
                },
            ],
            blocked: None,
            warnings: vec![],
            risk: crate::Risk::Low,
            allow_protected: false,
        };
        for (success, freed) in [(false, false), (false, true), (true, false)] {
            let mut completed = CompletedSupervisorStops::default();
            completed.remember(
                &original,
                &StopReport {
                    success,
                    freed,
                    ..Default::default()
                },
            );
            assert_eq!(completed.remaining(&original), original);
        }
        let mut completed = CompletedSupervisorStops::default();
        completed.remember(
            &original,
            &StopReport {
                success: true,
                freed: true,
                ..Default::default()
            },
        );
        let remaining = completed.remaining(&original);
        assert_eq!(remaining.steps, vec![original.steps[1].clone()]);
        assert_eq!(
            original.steps.len(),
            2,
            "history retains the reviewed command"
        );
    }
}
