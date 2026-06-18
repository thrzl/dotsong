<script lang="ts">
    import "../app.css";
    let { children } = $props();
</script>

{@render children?.()}

<style>
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
