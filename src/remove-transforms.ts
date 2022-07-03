import { XmlDocument, XmlElement } from 'xmldoc';
import SVG from './svg';
import svgpath from 'svgpath';

const transforms = [
  'translate',
  'scale',
  'matrix',
];

interface Transform {
  name: string,
  values: Array<number>
};

function parseTransforms(inputPath: XmlElement):Array<Transform> {
  const transformAttr = inputPath.attr.transform;
  
  if (!transformAttr) return [];

  let output: Array<Transform> = [];

  transforms.forEach(transform => {
    if (!transformAttr.includes(transform)) return;

    let stringValues: Array<string> = transformAttr.split(`${transform}(`)[1].split(')')[0].split(',');
    let numericValues: Array<number> = stringValues.map(x => parseFloat(x));

    output.push({
      name: transform,
      values: numericValues
    });
  });

  return output;
}

function applyTransform(inputPath: XmlElement, transforms: Array<Transform>, deleteTransform: Boolean = true): void {
  if (!inputPath.attr.d || transforms.length === 0) return;

  const path = svgpath(inputPath.attr.d);

  let transformedPath = path;

  transforms.forEach(transform => {
    switch(transform.name) {
      case 'translate':
        transformedPath = transformedPath.translate(transform.values[0], transform.values[1]);
        break;
      case 'scale':
        transformedPath = transformedPath.rotate(transform.values[0], transform.values[1]);
        break;
      case 'matrix':
        transformedPath = transformedPath.matrix(transform.values);
        break;
    }
  });

  inputPath.attr.d = transformedPath.toString();

  if (deleteTransform) delete inputPath.attr.transform;

  if (inputPath.children) {
    inputPath.eachChild(child => {
      applyTransform(child, transforms, false);
    });
  }
}

function modifyPaths(xmlNode: (XmlDocument | XmlElement)) {
  if (xmlNode.children.length === 0) return;

  xmlNode.eachChild((child) => {
    if (!child.name || child.name !== 'path') return;

    const transforms = parseTransforms(child);
    applyTransform(child, transforms, true);

    if (child.children.length > 0) modifyPaths(child);
  });
}

function removeTransforms(inputSvg: SVG) {
  const xmlDoc: XmlDocument = inputSvg.getXmlDoc();

  modifyPaths(xmlDoc);

  return new SVG(xmlDoc.toString());
}

export default removeTransforms;
