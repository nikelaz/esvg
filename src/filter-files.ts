function filterFiles(files: Array<string>) {
  return files.filter(x => x.includes('.svg'));
}

export default filterFiles;
