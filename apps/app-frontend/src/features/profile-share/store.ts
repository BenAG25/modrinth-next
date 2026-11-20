import { ref } from 'vue'

/** Instance the share dialog is open for. */
export const shareInstanceId = ref<string | null>(null)
/** Whether the import dialog is open. */
export const importOpen = ref(false)

export function openProfileShare(instanceId: string) {
	shareInstanceId.value = instanceId
}

export function openProfileImport() {
	importOpen.value = true
}
