import type { Meta, StoryObj } from "@storybook/sveltekit";
import DialogPreview from "./DialogPreview.svelte";

const meta = {
  title: "Overlays/Dialog",
  component: DialogPreview,
  tags: ["autodocs"],
} satisfies Meta<typeof DialogPreview>;

export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
