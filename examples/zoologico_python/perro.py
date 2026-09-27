from animal import Animal
from cuidador import Cuidador


class Perro(Animal):
    def __init__(self, nombre: str, edad: int, cuidador: Cuidador) -> None:
        super().__init__(nombre, edad)
        self.cuidador = cuidador
        self.juguetes: list[str] = []

    def sonido(self) -> str:
        return "Guau"
