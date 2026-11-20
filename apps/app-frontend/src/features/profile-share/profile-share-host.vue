<script setup lang="ts">
// Modrinth Next: mounts the share and import dialogs once for the whole app.
import ProfileImportModal from './profile-import-modal.vue'
import ProfileShareModal from './profile-share-modal.vue'

defineProps<{
	installInvite: (inviteId: string) => Promise<void> | void
}>()
</script>

<template>
	<ProfileShareModal />
	<ProfileImportModal :install-invite="installInvite" />
</template>
