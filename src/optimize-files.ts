import success from './success';
import optimizeFile from './optimize-file';

async function optimizeFiles(files: Array<string>, pretty: Boolean) {
  const operations = files.map(file => optimizeFile(file, pretty));
  await Promise.all(operations);
  success(files);
}

export default optimizeFiles;
