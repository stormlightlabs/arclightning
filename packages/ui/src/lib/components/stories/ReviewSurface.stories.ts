import type { Meta, StoryObj } from "@storybook/sveltekit";
import ReviewSurface from "./ReviewSurface.svelte";

const meta = {
  title: "Review/All states",
  component: ReviewSurface,
  tags: ["autodocs"],
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ReviewSurface>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Light: Story = { args: { theme: "light" } };
export const Dark: Story = { args: { theme: "dark" } };
