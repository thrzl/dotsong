<script lang="ts">
    import * as Field from "$lib/components/ui/field";
    import { Switch } from "$lib/components/ui/switch";
    let {
        value = $bindable(false),
        title,
        description,
        id,
        ariaLabel,
    }: {
        value: boolean;
        title: string;
        description: string;
        id: string;
        ariaLabel: string;
    } = $props();
</script>

<Field.Field orientation="horizontal">
    <Field.FieldContent>
        <Field.FieldTitle>{title}</Field.FieldTitle>
        <Field.FieldDescription>
            {description}
        </Field.FieldDescription>
    </Field.FieldContent>
    <Switch {id} bind:checked={value} aria-label={ariaLabel} />
</Field.Field>
