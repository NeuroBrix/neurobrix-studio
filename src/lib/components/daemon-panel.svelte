<script lang="ts">
  import { onMount } from "svelte";
  import { DaemonLifecycle } from "#lib/states/daemon.svelte.js";

  interface Props {
    /** The panel owns its lifecycle; tests inject one with scripted services. */
    daemon?: DaemonLifecycle;
  }

  let { daemon = new DaemonLifecycle() }: Props = $props();

  let modelId = $state("");

  onMount(() => {
    daemon.startPolling();
    return () => daemon.dispose();
  });

  async function submitStart(event: SubmitEvent) {
    event.preventDefault();
    const id = modelId.trim();
    if (id === "") return;
    await daemon.start(id);
  }
</script>

<section aria-label="Engine daemon">
  <h2 class="text-lg font-semibold tracking-tight">Engine daemon</h2>

  {#if daemon.status === undefined}
    <p>Reading daemon status…</p>
  {:else if daemon.lifecycle === "ready" && daemon.status.state === "ready"}
    <p>
      Serving {daemon.status.model} on {daemon.status.hardware}
      ({daemon.status.family ?? "unknown family"}, {daemon.status.mode} mode,
      {daemon.status.warm_serving ? "warm" : "cold"}
      serving). Engine {daemon.status.engine_version}, idle after
      {daemon.status.idle_timeout_secs}
      s.
    </p>
    <button type="button" onclick={() => daemon.stop()} disabled={daemon.isMutating}>
      Stop daemon
    </button>
  {:else if daemon.lifecycle === "external_daemon" && daemon.status.state === "external_daemon"}
    <p>
      A daemon this Studio session did not start is serving
      {daemon.status.model}
      on {daemon.status.hardware}
      (engine {daemon.status.engine_version}). It was started outside Studio and is left running.
    </p>
  {:else if daemon.lifecycle === "starting"}
    <p>
      Starting the daemon{daemon.status.state === "starting" ? ` for ${daemon.status.model}` : ""}…
    </p>
  {:else if daemon.lifecycle === "stopping"}
    <p>Stopping the daemon…</p>
  {:else if daemon.lifecycle === "stopped"}
    {#if daemon.status.state === "died" && daemon.status.expected}
      <p>
        The daemon stopped itself{daemon.status.idle_timeout_secs !== undefined
          ? ` after ${daemon.status.idle_timeout_secs} s idle`
          : ""}
        — its own expected exit.
      </p>
    {:else if daemon.status.state === "stopped" && daemon.status.stale_files}
      <p>
        No daemon is running, but files from a previous one remain. They are cleaned up on the next
        start.
      </p>
    {:else}
      <p>No daemon is running.</p>
    {/if}
    <form onsubmit={submitStart}>
      <label for="daemon-model">Model to serve</label>
      <input id="daemon-model" bind:value={modelId} disabled={daemon.isMutating} required>
      <button type="submit" disabled={daemon.isMutating || modelId.trim() === ""}>
        Start daemon
      </button>
    </form>
  {:else if daemon.lifecycle === "failed"}
    {#if daemon.status.state === "died"}
      <p>The daemon stopped unexpectedly.</p>
      {#if daemon.status.exit_code !== undefined}
        <p>Exit code {daemon.status.exit_code}.</p>
      {/if}
      {#if daemon.status.diagnostics}
        <pre>{daemon.status.diagnostics}</pre>
      {/if}
    {:else if daemon.status.state === "unavailable" || daemon.status.state === "contract_failure"}
      <p>Daemon problem: {daemon.status.reason}</p>
    {/if}
    <button type="button" onclick={() => daemon.refresh()}>Check again</button>
  {/if}

  {#if daemon.failure}
    <p role="alert">{daemon.failure}</p>
  {/if}
</section>
