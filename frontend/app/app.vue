<template>
  <NuxtLoadingIndicator />
  <NuxtLayout>
    <nav style="display: flex; gap: 10px">
      <NuxtLink to="/">Home</NuxtLink>
      <NuxtLink to="/search">Search</NuxtLink>
    </nav>
    <NuxtPage class="centered-body" />
  </NuxtLayout>
</template>

<style>
.centered-body {
  max-width: 512px;
  margin-inline: auto;
}
</style>
