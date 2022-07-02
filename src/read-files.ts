async function readFiles(files: Array<string>) {
  const svgFiles = files.filter(x => x.includes('.svg'));

  return svgFiles;
}

export default readFiles;
