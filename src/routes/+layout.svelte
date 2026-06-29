<script lang="ts">
    import "../app.css";
    import { ModeWatcher } from "mode-watcher";
    let { children } = $props();
</script>

<ModeWatcher/>
{@render children?.()}

<style>
    :global(html, body) {
        background: black /* don't flashbang people */ 
    }
    :global(*) {
        cursor: default;
    }
    :global(select, button, a) {
        cursor: pointer;
    }
    :global(textarea, input) {
        cursor: text;
    }
</style>
