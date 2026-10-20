// Failure alerting for the scheduled verification workflows (nightly.yml,
// verify.yml, differential.yml, mutation-ts.yml, supply-chain.yml).
//
// Run by .github/workflows/verification-alert.yml (workflow_run, after each of
// those workflows completes) through actions/github-script. For every job of a
// *scheduled* run that failed, timed out or was cancelled, opens an
// issue labelled `verification-failure`, or comments on the open one for the
// same job, with the run link and the failing step. One issue per job name
// (matrix jobs whose suffix rotates, such as the mutants shards, share one).
//
// Issues are never closed automatically: the random differential seeds and
// the mutants shards move every night, so a green run the next day does not
// mean yesterday's finding is fixed. Close the issue with the fix.

const LABEL = 'verification-failure';
const BAD = new Set(['failure', 'timed_out', 'cancelled', 'startup_failure']);

// Matrix values that change from one night to the next must not create a new
// issue per value.
const dedupeKey = (name) => name.replace(/^(mutants) \(.*\)$/, '$1');

module.exports = async ({ github, context, core }) => {
  const { owner, repo } = context.repo;
  const runId = Number(process.env.ALERT_RUN_ID);
  if (!runId) throw new Error('ALERT_RUN_ID is not set');
  const dryRun = process.env.ALERT_DRY_RUN === 'true';

  const { data: run } = await github.rest.actions.getWorkflowRun({ owner, repo, run_id: runId });
  const workflow = run.name;
  const runUrl = run.html_url;
  if (run.event !== 'schedule' && process.env.ALERT_ANY_EVENT !== 'true') {
    core.info(`${workflow} run ${runId} was triggered by ${run.event}, not schedule: no alert`);
    return;
  }

  const jobs = await github.paginate(github.rest.actions.listJobsForWorkflowRun, {
    owner, repo, run_id: runId, filter: 'latest', per_page: 100,
  });

  const failed = new Map(); // key -> [{job, step}]
  for (const job of jobs) {
    if (job.status !== 'completed' || !BAD.has(job.conclusion)) continue;
    const step = (job.steps || []).find((s) => BAD.has(s.conclusion));
    const key = dedupeKey(job.name);
    if (!failed.has(key)) failed.set(key, []);
    failed.get(key).push({ job, step });
  }

  if (failed.size === 0) {
    core.info(`${workflow}: no failed jobs in run ${runId}`);
    return;
  }

  if (!dryRun) {
    try {
      await github.rest.issues.getLabel({ owner, repo, name: LABEL });
    } catch (e) {
      if (e.status !== 404) throw e;
      await github.rest.issues.createLabel({
        owner, repo, name: LABEL, color: 'b60205',
        description: 'A scheduled verification job (fuzz, mutants, differential, proofs) failed',
      });
    }
  }

  const open = await github.paginate(github.rest.issues.listForRepo, {
    owner, repo, labels: LABEL, state: 'open', per_page: 100,
  });

  const sha = run.head_sha.slice(0, 10);
  for (const [key, entries] of failed) {
    const title = `Verification failure: ${workflow} / ${key}`;
    const lines = entries.map(({ job, step }) =>
      `- [${job.name}](${job.html_url}): **${job.conclusion}**` +
      (step ? ` in step \`${step.name}\`` : ' (no failing step reported: setup, runner loss or job timeout)'));
    const body = [
      `Run [${runId}](${runUrl}) of **${workflow}** on \`${run.head_branch}\` @ ${sha} (${run.event}, ${run.created_at}):`,
      '',
      ...lines,
      '',
      'Logs and uploaded reports (mismatch dumps, crash inputs, mutants.out) are on the run page.',
    ].join('\n');

    const existing = open.find((i) => i.title === title && !i.pull_request);
    if (dryRun) {
      core.info(`[dry run] ${existing ? `comment on #${existing.number}` : 'open issue'}: ${title}\n${body}`);
      continue;
    }
    if (existing) {
      await github.rest.issues.createComment({ owner, repo, issue_number: existing.number, body });
      core.warning(`${title}: commented on #${existing.number}`);
    } else {
      const created = await github.rest.issues.create({
        owner, repo, title, labels: [LABEL],
        body: `${body}\n\nOpened automatically by \`.github/scripts/verification-alert.cjs\`; ` +
          'later failures of the same job are added as comments. Close this issue with the fix ' +
          '(it is never closed automatically: seeds and shards move every night).',
      });
      core.warning(`${title}: opened #${created.data.number}`);
    }
  }
};
