<template>
  <Dialog v-model:visible="visible" modal :header="$t(album ? 'albums.rename' : 'albums.new')" :style="{ width: '25rem' }" @show="name = album?.name ?? ''">
    <form class="dialog-fields" @submit.prevent="save">
      <label for="album-name">{{ $t('albums.name') }}</label>
      <InputText id="album-name" v-model="name" size="small" fluid autofocus />
      <Button type="submit" :label="$t(album ? 'albums.rename' : 'albums.create')" size="small" :loading :disabled="!name.trim()" />
    </form>
  </Dialog>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Dialog from 'primevue/dialog'
import InputText from 'primevue/inputtext'
import Button from 'primevue/button'
import { api } from '@/services/api/http'
import type { Album } from '@/services/api-schema'

// Renames `album` when given, creates a new album of the caller's otherwise.
const props = defineProps<{ album?: Album }>()
const emit = defineEmits<{ saved: [Album] }>()
const visible = defineModel<boolean>('visible', { required: true })
const name = ref('')
const loading = ref(false)

const save = () => {
  loading.value = true
  const album = props.album
  const request = album
    ? api.renameAlbum(album.id, { name: name.value }).then(() => ({ ...album, name: name.value.trim() }))
    : api.createAlbum({ name: name.value }).then((response) => response.data)
  request
    .then((saved) => {
      visible.value = false
      emit('saved', saved)
    })
    .finally(() => {
      loading.value = false
    })
}
</script>

<style scoped>
.dialog-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 8px;
}
</style>
